//! The rig-powered assistant. It runs on its own tokio runtime. Each of its
//! tools turns into a [`Command`] that the [`Host`] applies to the app state,
//! exactly like the matching button, and whatever happens is reported back
//! to the UI as an [`AgentEvent`].
//!
//! The model is chosen at runtime as a rig provider reference such as
//! `openai:gpt-5.5`, `anthropic:claude-opus-5-5`, `gemini:gemini-2.5-pro` or
//! `ollama:llama3.2`; credentials come from that provider's usual environment
//! variable unless a key is entered in the panel.

use crate::state::Command;
use crate::tools::tool_specs;
use rig::completion::Message;
use rig::message::ToolName;
use rig::operation::Completion;
use rig::prelude::*;
use rig::providers::registry::{ProviderId, ProviderRef};
use rig::tool::{DynamicTool, ToolExecutionError, ToolOutput};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Notify;

/// Environment variable holding the provider reference to connect at startup.
pub const MODEL_ENV: &str = "JSON_DIFF_MODEL";

const PREAMBLE: &str = "\
You operate json-diff, a desktop app that compares two JSON documents side by side, \
like VS Code's diff editor. The left side is the original document and the right side \
the modified one. A folder explorer on the left has two halves: the top half browses the \
original folder and the bottom half the modified folder; when both are open, files with \
the same relative path are paired and marked modified, identical, only_left or only_right.
The app opens on the line diff (the `diff` view mode). The `tree` view mode shows both \
documents as one tree of keys, only the paths to changes, folded to the top level: good \
for an overview (get_tree lists its rows). In the diff, as in VS Code, changed lines are red on the original side and green on the \
modified side: a red line with no counterpart was removed, a green one was added, and a \
red block facing a green block is a value that changed in place (get_structural_changes \
tells the three apart). Uncoloured lines inside a changed block only differ in formatting.
Documents open folded: only top-level keys are shown, and `{…}` or `[…]` stands for a \
folded object or array. A folded key is red/green when something inside it changed. Use \
reveal_changes to open just the paths that lead to changes, or expand_node on a pointer \
(get_change lists the folded pointers in a block) before quoting nested lines.

Every button in the app is available to you as a tool, and your tool calls are visible \
to the user as they happen. Start with get_state to see what is open. Any original \
file can be compared with any modified file: select_file puts a file on one side (as a \
click in that side's explorer does). To compare two versions of a folder, open both and \
use list_pairs, then open_pair for files with the same relative path. To explain a diff, prefer \
get_structural_changes (JSON pointers with old and new values) and use next_change / \
get_change to show the user the corresponding lines. Answer concisely and refer to \
values by their JSON pointer. The user can ignore changes to keys (right-click, or \
ignore_changes when they ask); those changes are left out everywhere, and you say so \
when you summarise. Only use export_pdf when the user explicitly asks for a \
PDF; it asks them where to save it.";

/// The tool results shown in the chat log are cut to this many characters.
const LOGGED_RESULT_CHARS: usize = 2_000;

/// What the assistant needs from the app.
pub trait Host: Send + Sync + 'static {
    /// Applies a tool's command to the app state, exactly like the button
    /// it stands for.
    fn apply(&self, command: Command) -> Result<Value, String>;
    /// Tells the UI what the assistant did.
    fn emit(&self, event: AgentEvent);
}

pub enum AgentRequest {
    Connect {
        model: String,
        api_key: Option<String>,
    },
    Prompt {
        text: String,
        /// The AGENTS.md section of the system prompt, as of this message
        /// (see [`crate::agents_md::AgentsMd::prompt_section`]).
        instructions: String,
    },
    /// Forget the conversation.
    Reset,
}

/// What the chat panel shows, sent to the frontend as the `agent` event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Connected {
        model: String,
    },
    /// A tool ran; `result` or `error` holds what it returned.
    Tool {
        name: String,
        arguments: String,
        result: Option<String>,
        error: Option<String>,
    },
    Reply {
        text: String,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

pub struct AgentHandle {
    requests: tokio::sync::mpsc::UnboundedSender<AgentRequest>,
    cancel: Arc<Notify>,
}

impl AgentHandle {
    pub fn send(&self, request: AgentRequest) {
        // The worker only stops when the handle is dropped.
        let _ = self.requests.send(request);
    }

    /// Abort the prompt being answered, if any.
    pub fn cancel(&self) {
        // Wakes only a prompt that is waiting; nothing is stored for later.
        self.cancel.notify_waiters();
    }
}

/// Starts the agent worker on its own thread and runtime.
pub fn spawn(host: Arc<dyn Host>) -> AgentHandle {
    let (requests, request_rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel = Arc::new(Notify::new());
    let worker = Worker {
        host,
        cancel: cancel.clone(),
    };
    std::thread::Builder::new()
        .name("agent".into())
        .spawn(move || {
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(worker.run(request_rx)),
                Err(error) => worker.host.emit(AgentEvent::Failed {
                    message: format!("cannot start async runtime: {error}"),
                }),
            }
        })
        .expect("spawning the agent thread");
    AgentHandle { requests, cancel }
}

/// Every provider this build can connect to, as the selection a reference
/// starts with: the vendor name (`openai`), or `vendor/format` for a vendor
/// served in several API formats, where the vendor alone is ambiguous.
pub fn known_providers() -> Vec<String> {
    let mut selections: Vec<String> = ProviderId::all()
        .map(|id| {
            if ProviderId::resolve(id.vendor()).is_ok() {
                id.vendor().to_owned()
            } else {
                id.to_string()
            }
        })
        .collect();
    selections.sort_unstable();
    selections.dedup();
    selections
}

/// The vendor a provider selection names: `minimax/openai` → `minimax`.
fn vendor_of(selection: &str) -> &str {
    selection
        .split_once('/')
        .map_or(selection, |(vendor, _)| vendor)
}

/// One entry of the settings panel's model list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelChoice {
    /// The id sent to the provider, e.g. `gpt-5.5`.
    pub id: String,
    /// The catalog's human-readable name, e.g. `GPT-5.5`.
    pub name: String,
}

/// The models rig's built-in catalog lists for provider `selection` that
/// can drive the app, i.e. call tools, newest-looking ids first. Deprecated
/// models are left out. Empty for providers the catalog does not cover
/// (local servers such as ollama), where the id is typed instead.
pub fn catalog_models(selection: &str) -> Vec<ModelChoice> {
    let vendor = vendor_of(selection);
    let mut models: Vec<ModelChoice> = rig::catalog::Catalog::builtin()
        .iter()
        .filter(|spec| spec.provider.vendor() == vendor && spec.tools && !spec.deprecated)
        .map(|spec| ModelChoice {
            id: spec.id.clone(),
            name: spec.display_name.clone(),
        })
        .collect();
    models.sort_by(|a, b| b.id.cmp(&a.id));
    models
}

/// The environment variable provider `selection` reads its API key from by
/// default.
pub fn api_key_env(selection: &str) -> Option<&'static str> {
    ProviderId::resolve(selection.trim()).ok()?.api_key_env()
}

#[derive(Clone)]
struct Worker {
    host: Arc<dyn Host>,
    cancel: Arc<Notify>,
}

impl Worker {
    fn emit(&self, event: AgentEvent) {
        self.host.emit(event);
    }

    async fn run(self, mut requests: tokio::sync::mpsc::UnboundedReceiver<AgentRequest>) {
        let mut connected: Option<(DynModel<Completion>, Agent)> = None;
        // The AGENTS.md section the current agent was built with.
        let mut current_instructions = String::new();
        let mut history: Vec<Message> = Vec::new();
        while let Some(request) = requests.recv().await {
            match request {
                AgentRequest::Connect { model, api_key } => {
                    match self.model(&model, api_key).and_then(|completion| {
                        let agent = self.agent(completion.clone(), &current_instructions)?;
                        Ok((completion, agent))
                    }) {
                        Ok(built) => {
                            connected = Some(built);
                            history.clear();
                            self.emit(AgentEvent::Connected { model });
                        }
                        Err(message) => self.emit(AgentEvent::Failed { message }),
                    }
                }
                AgentRequest::Prompt {
                    text: prompt,
                    instructions,
                } => {
                    let Some((model, agent)) = &mut connected else {
                        self.emit(AgentEvent::Failed {
                            message: "Connect a model first.".into(),
                        });
                        continue;
                    };
                    // Instructions changed (a folder was opened, a file
                    // added or switched off): same model and conversation,
                    // new system prompt.
                    if instructions != current_instructions {
                        match self.agent(model.clone(), &instructions) {
                            Ok(rebuilt) => {
                                *agent = rebuilt;
                                current_instructions = instructions;
                            }
                            Err(message) => {
                                self.emit(AgentEvent::Failed { message });
                                continue;
                            }
                        }
                    }
                    // Register before running so `notify_waiters` sees us.
                    let cancelled = self.cancel.notified();
                    tokio::pin!(cancelled);
                    cancelled.as_mut().enable();
                    let event = tokio::select! {
                        result = agent.chat(prompt, &mut history) => match result {
                            Ok(response) => AgentEvent::Reply { text: response.output() },
                            Err(error) => AgentEvent::Failed { message: error.to_string() },
                        },
                        () = &mut cancelled => AgentEvent::Cancelled,
                    };
                    self.emit(event);
                }
                AgentRequest::Reset => history.clear(),
            }
        }
    }

    fn model(&self, model: &str, api_key: Option<String>) -> Result<DynModel<Completion>, String> {
        let reference = ProviderRef::parse(model.trim()).map_err(|error| {
            format!("'{model}' is not a model reference like openai:gpt-5.5 ({error})")
        })?;
        match api_key {
            Some(key) => Ok(
                reference.completion_model_with(key, rig::http_client::ReqwestClient::default())
            ),
            None => reference
                .completion_model()
                .map_err(|error| error.to_string()),
        }
    }

    /// Runs one tool call through the host and logs it in the chat.
    fn call(
        &self,
        name: &'static str,
        arguments: &Value,
        command: Command,
    ) -> Result<Value, String> {
        let result = self.host.apply(command);
        self.emit(AgentEvent::Tool {
            name: name.to_owned(),
            arguments: abbreviate(&arguments.to_string(), 80),
            result: result.as_ref().ok().map(|value| {
                abbreviate(
                    &serde_json::to_string_pretty(value).unwrap_or_default(),
                    LOGGED_RESULT_CHARS,
                )
            }),
            error: result.as_ref().err().cloned(),
        });
        result
    }

    /// The assistant on `model`, with one tool per app command, following
    /// the AGENTS.md `instructions` section.
    fn agent(&self, model: DynModel<Completion>, instructions: &str) -> Result<Agent, String> {
        let tools = tool_specs()
            .into_iter()
            .map(|spec| {
                let name = ToolName::new(spec.name).map_err(|error| error.to_string())?;
                let worker = self.clone();
                let callback = move |arguments: Value| {
                    let worker = worker.clone();
                    let parsed = (spec.parse)(arguments.clone());
                    Box::pin(async move {
                        let command = parsed.map_err(|error| {
                            let message = format!("invalid arguments: {error}");
                            ToolExecutionError::invalid_args(message.clone())
                                .with_model_feedback(message)
                        })?;
                        match worker.call(spec.name, &arguments, command) {
                            Ok(value) => Ok(ToolOutput::json(value)),
                            Err(message) => Err(ToolExecutionError::other(message.clone())
                                .with_model_feedback(message)),
                        }
                    }) as _
                };
                Ok(DynamicTool::new(
                    name,
                    spec.description,
                    spec.parameters,
                    callback,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(AgentBuilder::new(model)
            .name("json-diff assistant")
            .preamble(format!("{PREAMBLE}{instructions}"))
            .default_max_turns(30)
            .dynamic_tools(tools)
            .build())
    }
}

/// `text` cut to about `limit` characters, on a character boundary.
fn abbreviate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        text.to_owned()
    } else {
        format!("{}…", &text[..text.floor_char_boundary(limit)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use rig::completion::{CompletionRequest, CompletionResponse, Usage};
    use rig::driver::{Exchange, Model, Opened, Opening, Transport};
    use rig::error::ProviderError;
    use rig::message::{AssistantContent, Origin, ToolCall, ToolFunction, UserContent};
    use rig::test_utils::{MockFrame, MockScript};

    /// Every provider in the dropdown resolves, and each catalog model it
    /// offers forms a reference rig accepts.
    #[test]
    fn model_dropdowns_offer_only_connectable_choices() {
        let providers = known_providers();
        assert!(providers.iter().any(|p| p == "openai") && providers.iter().any(|p| p == "ollama"));
        for vendor in &providers {
            assert!(ProviderId::resolve(vendor).is_ok(), "{vendor}");
            let models = catalog_models(vendor);
            eprintln!(
                "{vendor} ({}): {:?}",
                models.len(),
                models.iter().map(|m| &m.id).take(8).collect::<Vec<_>>()
            );
            for model in models {
                let reference = format!("{vendor}:{}", model.id);
                assert!(ProviderRef::parse(&reference).is_ok(), "{reference}");
            }
        }
        assert!(!catalog_models("openai").is_empty());
    }

    /// A model that asks for `set_options` once, then reports the tool
    /// result it was given.
    #[derive(Clone)]
    struct Scripted;

    impl Transport<MockScript> for Scripted {
        fn send(&self, request: CompletionRequest, _exchange: Exchange) -> Opening<MockFrame> {
            let tool_result = request
                .chat_history
                .iter()
                .find_map(|message| match message {
                    Message::User { content } => content.iter().find_map(|item| match item {
                        UserContent::ToolResult(result) => serde_json::to_string(result).ok(),
                        _ => None,
                    }),
                    _ => None,
                });
            let answer = match tool_result {
                Some(result) => AssistantContent::text(format!("tool said: {result}")),
                None => AssistantContent::ToolCall(ToolCall::from_wire(
                    "call-1",
                    ToolFunction::new(
                        ToolName::new("set_options").expect("valid name"),
                        serde_json::json!({ "sort_keys": true }),
                    ),
                )),
            };
            let response = CompletionResponse::new(
                vec![answer],
                Usage::new(),
                Origin::new("test.api", "scripted", ""),
                Value::Null,
            );
            Opening::ready(Opened::new(futures::stream::iter([
                Ok::<_, ProviderError>(MockFrame::Response(Box::new(response))),
            ])))
        }
    }

    /// Applies commands to a real `AppState` and records what is emitted.
    #[derive(Default)]
    struct FakeHost {
        state: std::sync::Mutex<AppState>,
        events: std::sync::Mutex<Vec<AgentEvent>>,
    }

    impl Host for FakeHost {
        fn apply(&self, command: Command) -> Result<Value, String> {
            self.state.lock().unwrap().apply(command)
        }

        fn emit(&self, event: AgentEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_calls_reach_the_app_state_and_are_logged() {
        let host = Arc::new(FakeHost::default());
        let worker = Worker {
            host: host.clone(),
            cancel: Arc::new(Notify::new()),
        };
        let agent = worker
            .agent(Model::new(MockScript::new("scripted"), Scripted).into(), "")
            .unwrap();

        let mut history = Vec::new();
        let response = agent.chat("sort the keys", &mut history).await.unwrap();

        assert!(
            host.state.lock().unwrap().options.sort_keys,
            "the command reached AppState"
        );
        let events = host.events.lock().unwrap();
        let [AgentEvent::Tool { name, error, .. }] = events.as_slice() else {
            panic!("expected one logged tool call, got {events:?}");
        };
        assert_eq!(name, "set_options");
        assert_eq!(*error, None);
        let output = response.output();
        assert!(
            output.contains("sort_keys"),
            "the result reached the model: {output}"
        );
        assert!(!history.is_empty());
    }

    #[test]
    fn long_results_are_abbreviated_on_a_char_boundary() {
        assert_eq!(abbreviate("short", 10), "short");
        assert_eq!(abbreviate("São Paulo", 2), "S…");
    }
}

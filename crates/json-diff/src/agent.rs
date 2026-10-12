//! The rig-powered assistant. It runs on its own tokio runtime; its tools do
//! not touch app state directly but send each [`Command`] to the UI thread,
//! which applies it exactly like a button press and sends back the result.
//!
//! The model is chosen at runtime as a rig provider reference such as
//! `openai:gpt-5.5`, `anthropic:claude-opus-5-5`, `gemini:gemini-2.5-pro` or
//! `ollama:llama3.2`; credentials come from that provider's usual environment
//! variable unless a key is entered in the panel.

use crate::state::Command;
use crate::tools::tool_specs;
use eframe::egui;
use rig::completion::Message;
use rig::message::ToolName;
use rig::operation::Completion;
use rig::prelude::*;
use rig::providers::registry::{ProviderId, ProviderRef};
use rig::tool::{DynamicTool, ToolExecutionError, ToolOutput};
use serde_json::Value;
use std::sync::{Arc, mpsc};
use tokio::sync::{Notify, oneshot};

/// Environment variable holding the provider reference to connect at startup.
pub const MODEL_ENV: &str = "JSON_DIFF_MODEL";

const PREAMBLE: &str = "\
You operate json-diff, a desktop app that compares two JSON documents side by side, \
like VS Code's diff editor. The left side is the original document and the right side \
the modified one. A folder explorer on the left has two halves: the top half browses the \
original folder and the bottom half the modified folder; when both are open, files with \
the same relative path are paired and marked modified, identical, only_left or only_right.
In the diff, as in VS Code, changed lines are red on the original side and green on the \
modified side: a red line with no counterpart was removed, a green one was added, and a \
red block facing a green block is a value that changed in place (get_structural_changes \
tells the three apart). Uncoloured lines inside a changed block only differ in formatting.
Documents open folded: only top-level keys are shown, and `{…}` or `[…]` stands for a \
folded object or array. A folded key is red/green when something inside it changed. Use \
reveal_changes to open just the paths that lead to changes, or expand_node on a pointer \
(get_change lists the folded pointers in a block) before quoting nested lines.

Every button in the app is available to you as a tool, and your tool calls are visible \
to the user as they happen. Start with get_state to see what is open. To compare \
folders, open both and use list_pairs, then open_pair. To explain a diff, prefer \
get_structural_changes (JSON pointers with old and new values) and use next_change / \
get_change to show the user the corresponding lines. Answer concisely and refer to \
values by their JSON pointer.";

pub enum AgentRequest {
    Connect {
        model: String,
        api_key: Option<String>,
    },
    Prompt(String),
    /// Forget the conversation.
    Reset,
}

pub enum AgentEvent {
    /// A tool wants `command` applied; the UI must answer on `reply`.
    ToolCall {
        tool: &'static str,
        arguments: Value,
        command: Command,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Connected {
        model: String,
    },
    Reply(String),
    Failed(String),
    Cancelled,
}

pub struct AgentHandle {
    requests: tokio::sync::mpsc::UnboundedSender<AgentRequest>,
    cancel: Arc<Notify>,
    pub events: mpsc::Receiver<AgentEvent>,
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

/// Starts the agent worker thread. `ctx` is used to wake the UI whenever an
/// event is waiting.
pub fn spawn(ctx: egui::Context) -> AgentHandle {
    let (requests, request_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, events) = mpsc::channel();
    let cancel = Arc::new(Notify::new());
    let worker = Worker {
        events: event_tx,
        ctx,
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
                Err(error) => worker.emit(AgentEvent::Failed(format!(
                    "cannot start async runtime: {error}"
                ))),
            }
        })
        .expect("spawning the agent thread");
    AgentHandle {
        requests,
        cancel,
        events,
    }
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
#[derive(Debug, Clone, PartialEq)]
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

/// Splits a `vendor:model` reference into its provider and model id; a
/// reference without a model leaves the id empty.
pub fn split_reference(reference: &str) -> (String, String) {
    match reference.trim().split_once(':') {
        Some((vendor, model)) => (vendor.to_owned(), model.to_owned()),
        None => (reference.trim().to_owned(), String::new()),
    }
}

/// The environment variable provider `selection` reads its API key from by
/// default.
pub fn api_key_env(selection: &str) -> Option<&'static str> {
    ProviderId::resolve(selection.trim()).ok()?.api_key_env()
}

#[derive(Clone)]
struct Worker {
    events: mpsc::Sender<AgentEvent>,
    ctx: egui::Context,
    cancel: Arc<Notify>,
}

impl Worker {
    fn emit(&self, event: AgentEvent) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    async fn run(self, mut requests: tokio::sync::mpsc::UnboundedReceiver<AgentRequest>) {
        let mut agent: Option<Agent> = None;
        let mut history: Vec<Message> = Vec::new();
        while let Some(request) = requests.recv().await {
            match request {
                AgentRequest::Connect { model, api_key } => match self.build(&model, api_key) {
                    Ok(built) => {
                        agent = Some(built);
                        history.clear();
                        self.emit(AgentEvent::Connected { model });
                    }
                    Err(error) => self.emit(AgentEvent::Failed(error)),
                },
                AgentRequest::Prompt(prompt) => {
                    let Some(agent) = &agent else {
                        self.emit(AgentEvent::Failed("Connect a model first.".into()));
                        continue;
                    };
                    // Register before running so `notify_waiters` sees us.
                    let cancelled = self.cancel.notified();
                    tokio::pin!(cancelled);
                    cancelled.as_mut().enable();
                    let event = tokio::select! {
                        result = agent.chat(prompt, &mut history) => match result {
                            Ok(response) => AgentEvent::Reply(response.output()),
                            Err(error) => AgentEvent::Failed(error.to_string()),
                        },
                        () = &mut cancelled => AgentEvent::Cancelled,
                    };
                    self.emit(event);
                }
                AgentRequest::Reset => history.clear(),
            }
        }
    }

    fn build(&self, model: &str, api_key: Option<String>) -> Result<Agent, String> {
        let reference = ProviderRef::parse(model.trim()).map_err(|error| {
            format!("'{model}' is not a model reference like openai:gpt-5.5 ({error})")
        })?;
        let completion_model = match api_key {
            Some(key) => {
                reference.completion_model_with(key, rig::http_client::ReqwestClient::default())
            }
            None => reference
                .completion_model()
                .map_err(|error| error.to_string())?,
        };
        self.agent(completion_model)
    }

    /// The assistant on `model`, with one tool per app command.
    fn agent(&self, model: DynModel<Completion>) -> Result<Agent, String> {
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
                        let (reply, answer) = oneshot::channel();
                        worker.emit(AgentEvent::ToolCall {
                            tool: spec.name,
                            arguments,
                            command,
                            reply,
                        });
                        match answer.await {
                            Ok(Ok(value)) => Ok(ToolOutput::json(value)),
                            Ok(Err(message)) => Err(ToolExecutionError::other(message.clone())
                                .with_model_feedback(message)),
                            Err(_) => Err(ToolExecutionError::cancelled("the app closed")),
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
            .preamble(PREAMBLE)
            .default_max_turns(30)
            .dynamic_tools(tools)
            .build())
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
    use std::time::Duration;

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
        assert_eq!(
            split_reference("ollama:qwen3:4b"),
            ("ollama".into(), "qwen3:4b".into())
        );
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

    #[tokio::test(flavor = "multi_thread")]
    async fn tool_calls_are_applied_by_the_ui_and_answered() {
        let (events, received) = mpsc::channel();
        let worker = Worker {
            events,
            ctx: egui::Context::default(),
            cancel: Arc::new(Notify::new()),
        };
        let agent = worker
            .agent(Model::new(MockScript::new("scripted"), Scripted).into())
            .unwrap();

        // Stands in for the UI thread: apply the one expected tool call.
        let ui = std::thread::spawn(move || {
            let mut state = AppState::default();
            let event = received
                .recv_timeout(Duration::from_secs(10))
                .expect("a tool call");
            let AgentEvent::ToolCall {
                tool,
                command,
                reply,
                ..
            } = event
            else {
                panic!("expected a tool call");
            };
            let _ = reply.send(state.apply(command));
            (tool, state.options.sort_keys)
        });

        let mut history = Vec::new();
        let response = agent.chat("sort the keys", &mut history).await.unwrap();
        let (tool, sorted) = ui.join().unwrap();

        assert_eq!(tool, "set_options");
        assert!(sorted, "the command reached AppState");
        let output = response.output();
        assert!(
            output.contains("sort_keys"),
            "the result reached the model: {output}"
        );
        assert!(!history.is_empty());
    }
}

// The JavaScript interface wasm-bindgen generates for crates/json-diff-web
// (built into ui/wasm/ by `npm run wasm`), declared here so the frontend
// typechecks without building it. Keep in step with that crate.

declare module "#wasm" {
  export class App {
    constructor(settings?: string | null);
    free(): void;
    view(): string;
    /** Throws a message for an unknown command or bad arguments. */
    run(name: string, args: string): string;
    edit_text(side: string, text: string): string;
    open_text(side: string, name: string, text: string): string;
    key_at(side: string, line: number): string;
    export_pdf(context: number | null | undefined, paper: string, now: number): PdfFile;
    pdf_name(): string;
    settings(): string;
    ui_settings(): string;
    set_ui_settings(ui: string): void;
    whats_new(): string;
    release_notes(): string;
    static version(): string;
  }

  export class PdfFile {
    free(): void;
    readonly bytes: Uint8Array;
    readonly pages: number;
  }

  /** Loads and instantiates the module. */
  export default function init(options?: { module_or_path?: string | URL }): Promise<unknown>;
}

declare module "#notices" {
  const text: string;
  export default text;
}

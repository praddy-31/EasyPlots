// Typed wrappers for the Rust functions in app/src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export interface UiTheme {
  name: string;
  mode: "dark" | "light";
  colors: Record<string, string>;
}
/** Palette's glass for the launcher (`palette glass easyplots`). */
export interface Glass {
  kind: "solid" | "blurred" | "clear";
  opacity: number;
  blur: number;
}
/** The window's opacity and blur, from Palette's glass or config.toml. */
export interface Look {
  opacity: number;
  blur: number;
  palette: Glass | null;
  /** Below 40 % solid: text may be hard to read. */
  faint: boolean;
}
export interface ThemeInfo {
  dark: UiTheme;
  light: UiTheme;
  source: string;
  look: Look;
  stamp: number;
}
export interface UiSettings {
  opacity: number;
  font: string;
  font_size: number;
  hints: boolean;
  theme: ThemeInfo;
}
export interface LauncherInfo {
  mode: "picker" | "command";
  history: string[];
  warnings: string[];
  data_dir: string;
  scanning: boolean;
  ui: UiSettings;
  /** The format ↵ saves in (output.format). */
  format: string;
  live_preview: boolean;
  presets: Preset[];
  /** Saved styles (--style=NAME). */
  styles: string[];
  /** Bare word options (markers, logy, dropna…). */
  flags?: string[];
}
export interface Preset {
  name: string;
  command: string;
}
export interface FileHit {
  display: string;
  insert: string;
  /** The file as typed, and the sheet when the entry is one sheet of a workbook. */
  file: string;
  sheet: string | null;
  guess: boolean;
  /** A folder of data files: picked, it gives one panel per file. */
  folder?: boolean;
}
/** The first rows of a file, shown beside the list while it is highlighted. */
export interface Peek {
  file: string;
  rows: number;
  names: string[];
  kinds: ("numbers" | "dates" | "text")[];
  cells: string[][];
}
export interface ColumnInfo {
  name: string;
  insert: string;
  kind: "numbers" | "dates" | "text";
  /** e.g. "12.1 – 31.4 · mean 22 · 3 blanks" (not sent with command-mode suggestions). */
  summary?: string;
}
export interface Columns {
  file: string;
  rows: number;
  columns: ColumnInfo[];
}
export interface Suggestion {
  label: string;
  insert: string;
  detail: string;
}
export interface Completion {
  start: number;
  end: number;
  items: Suggestion[];
}
/** A plot advisor hint; `fix` is the whole command with the option added (⇥). */
export interface Advice {
  text: string;
  fix: string | null;
}
export interface Preview {
  image: string;
  millis: number;
  advice: Advice[];
}
export interface SaveResult {
  path: string;
  shown_path: string;
  after_save: "thumbnail" | "notify" | "open" | "reveal" | "none";
}
export interface PlotError {
  message: string;
  start: number | null;
  end: number | null;
  /** A ready fix: replace start..end of the command with text (⇥). */
  fix?: { start: number; end: number; text: string } | null;
}

export const launcherInfo = () => invoke<LauncherInfo>("launcher_info");
export const currentTheme = () => invoke<ThemeInfo>("current_theme");
export const searchFiles = (query: string) => invoke<FileHit[]>("search_files", { query });
export const listColumns = (file: string, sheet: string | null = null) =>
  invoke<Columns>("list_columns", { file, sheet });
export const peekFile = (file: string, sheet: string | null = null) =>
  invoke<Peek>("peek_file", { file, sheet });
export const complete = (input: string, cursor: number) =>
  invoke<Completion>("complete_command", { input, cursor });
export const recallPlot = (plot: string) => invoke<string>("recall_plot", { plot });
export const preview = (command: string) => invoke<Preview>("preview", { command });
export const endPreview = () => invoke<void>("end_preview");
export const savePreview = (format: string | null = null) => invoke<SaveResult>("save_preview", { format });
export const previewThumb = (command: string) => invoke<string>("preview_thumb", { command });
export const holdOpen = (on: boolean) => invoke<void>("hold_open", { on });
export const setPinned = (on: boolean) => invoke<void>("set_pinned", { on });
export const savePreset = (name: string) => invoke<void>("save_preset", { name });
/** Keep the previewed plot's look as a style; returns where it was written. */
export const saveStyle = (name: string) => invoke<string>("save_style", { name });
export const applyPreset = (name: string, file: string) => invoke<string>("apply_preset", { name, file });
export const savePreviewAs = () => invoke<SaveResult | null>("save_preview_as");
export const copyPreview = () => invoke<void>("copy_preview");
export const copyPreviewData = () => invoke<void>("copy_preview_data");
export const openPlot = (path: string, reveal: boolean) => invoke<void>("open_plot", { path, reveal });
export const hideLauncher = () => invoke<void>("hide_launcher");
export interface RecentPlot {
  path: string;
  name: string;
  /** Seconds since 1970. */
  modified: number;
  command: string;
}
/** The newest 60 plots EasyPlots saved in the plots folder. */
export const recentPlots = () => invoke<RecentPlot[]>("recent_plots");
/** A small picture of a saved plot (a PNG data URL). */
export const plotThumb = (path: string) => invoke<string>("plot_thumb", { path });
export const growLauncher = () => invoke<void>("grow_launcher");
/** The clipboard's table, if it holds one (rows × columns). */
export const clipboardTable = () => invoke<{ rows: number; columns: number } | null>("clipboard_table");
/** Save the clipboard's table in the pasted-data folder; returns the file as typed. */
export const saveClipboard = () => invoke<string>("save_clipboard");

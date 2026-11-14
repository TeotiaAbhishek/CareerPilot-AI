// The shapes the worker sends to the Rust API (as JSON).

export type FieldKind =
  | "text"
  | "textarea"
  | "email"
  | "tel"
  | "number"
  | "date"
  | "url"
  | "select"
  | "radio"
  | "checkbox"
  | "file";

export interface Field {
  /** Stable id for this page, e.g. "f3". Pass it back to fill. */
  id: string;
  kind: FieldKind;
  /** The question or label, as the page shows it. */
  label: string;
  required: boolean;
  /** The choices, for select, radio and checkbox ("Yes" / "No"). */
  options: string[];
  /** What's in it now ("" when empty). */
  value: string;
}

export type ButtonKind = "submit" | "next" | "back" | "other";

export interface Button {
  id: string;
  text: string;
  /** The HTML type attribute, e.g. "submit". */
  type: string;
}

export interface PageSnapshot {
  url: string;
  title: string;
  headings: string[];
  fields: Field[];
  buttons: Button[];
  /** Visible error messages, e.g. "This field is required". */
  errors: string[];
  /** A visible CAPTCHA challenge. The agent never tries to solve one. */
  captcha: boolean;
  /** A visible password box: this is a login or sign-up page. */
  password: boolean;
}

/** A snapshot as the API receives it: buttons come with their kind. */
export interface Snapshot extends Omit<PageSnapshot, "buttons"> {
  buttons: (Button & { kind: ButtonKind })[];
}

export interface FillRequest {
  id: string;
  value: string;
}

export interface FillResult {
  id: string;
  ok: boolean;
  error?: string;
}

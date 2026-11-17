// Runs INSIDE the web page (Playwright sends this function's source to the
// browser), so it can't use anything imported in this file: everything it
// needs is defined inside it.
//
// It reads the form on the page into a plain list the agent can reason about,
// and tags each element with data-cp / data-cp-btn so later fill and click
// calls can find it again. The ids stay the same across snapshots.

import type { PageSnapshot } from "./types.ts";

export function readPage(): PageSnapshot {
  const w = window as unknown as { __cpNext?: number };
  const nextId = (prefix: string) => {
    w.__cpNext = (w.__cpNext ?? 0) + 1;
    return `${prefix}${w.__cpNext}`;
  };

  const clean = (s: string | null | undefined) =>
    (s ?? "").replace(/\s+/g, " ").trim().slice(0, 300);

  const isVisible = (el: Element | null): boolean => {
    if (!el) return false;
    const r = el.getBoundingClientRect();
    const s = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
  };

  // Text of a node without the text of any form controls inside it.
  const textWithoutControls = (node: Element) => {
    const copy = node.cloneNode(true) as Element;
    copy.querySelectorAll("input, select, textarea, button, option").forEach((c) => c.remove());
    return clean(copy.textContent);
  };

  const byIds = (ids: string | null) =>
    clean(
      (ids ?? "")
        .split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent ?? "")
        .join(" "),
    );

  const labelOf = (el: HTMLElement): string => {
    const labelledBy = byIds(el.getAttribute("aria-labelledby"));
    if (labelledBy) return labelledBy;
    if (el.id) {
      const label = document.querySelector(`label[for="${CSS.escape(el.id)}"]`);
      if (label) {
        const text = textWithoutControls(label);
        if (text) return text;
      }
    }
    const wrapping = el.closest("label");
    if (wrapping) {
      const text = textWithoutControls(wrapping);
      if (text) return text;
    }
    return clean(
      el.getAttribute("aria-label") ||
        el.getAttribute("placeholder") ||
        el.getAttribute("title") ||
        el.getAttribute("name"),
    );
  };

  // The question a group of radios (or a checkbox) answers.
  const questionOf = (inputs: HTMLElement[]): string => {
    const first = inputs[0];
    const group = first.closest('[role="radiogroup"], [role="group"]');
    if (group) {
      const text =
        byIds(group.getAttribute("aria-labelledby")) || clean(group.getAttribute("aria-label"));
      if (text) return text;
    }
    const legend = first.closest("fieldset")?.querySelector("legend");
    if (legend) return clean(legend.textContent);
    // The smallest element holding the whole group, then the text just before it.
    let box: Element | null = first.parentElement;
    while (box && !inputs.every((i) => box!.contains(i))) box = box.parentElement;
    for (let node: Element | null = box; node && node !== document.body; node = node.parentElement) {
      let prev = node.previousElementSibling;
      while (prev && !clean(prev.textContent)) prev = prev.previousElementSibling;
      if (prev && !prev.querySelector("input, select, textarea")) return clean(prev.textContent);
      if (prev) break;
    }
    return "";
  };

  // The last heading before an element, looking a few levels up: "Resumé"
  // for SEEK's Upload button and its hidden file box.
  const headingBefore = (el: Element): string => {
    for (let node = el.parentElement, depth = 0; node && depth < 4; node = node.parentElement, depth++) {
      const before = Array.from(
        node.querySelectorAll('legend, h1, h2, h3, h4, h5, h6, [role="heading"]'),
      ).filter((h) => !h.contains(el) && h.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING);
      const text = clean(before.at(-1)?.textContent);
      if (text) return text;
    }
    return "";
  };

  const isRequired = (el: HTMLElement, label: string) =>
    (el as HTMLInputElement).required ||
    el.getAttribute("aria-required") === "true" ||
    /\*\s*$|\(required\)/i.test(label);

  const fields: PageSnapshot["fields"] = [];
  const seenGroups = new Set<string>();
  const controls = Array.from(
    document.querySelectorAll<HTMLElement>("input, select, textarea"),
  );

  for (const el of controls) {
    const input = el as HTMLInputElement;
    const type = (input.type || el.tagName).toLowerCase();
    if (["hidden", "submit", "button", "image", "reset", "password", "search"].includes(type)) {
      continue;
    }
    if (input.disabled || el.closest("[hidden], [aria-hidden='true']")) continue;

    if (type === "radio") {
      const name = input.name;
      const key = `radio:${name || nextId("anon")}`;
      if (seenGroups.has(key)) continue;
      seenGroups.add(key);
      const group = name
        ? Array.from(
            document.querySelectorAll<HTMLInputElement>(
              `input[type="radio"][name="${CSS.escape(name)}"]`,
            ),
          )
        : [input];
      const shown = group.filter(
        (r) => isVisible(r) || isVisible(r.closest("label")) || isVisible(r.parentElement),
      );
      if (shown.length === 0) continue;
      // A radio added later (a new upload) joins the group's existing id.
      const id = shown.find((r) => r.dataset.cp)?.dataset.cp || nextId("f");
      const options = shown.map((r) => {
        r.dataset.cp = id;
        const label = labelOf(r) || clean(r.value);
        r.dataset.cpLabel = label;
        return label;
      });
      const question = questionOf(shown);
      const checked = shown.find((r) => r.checked);
      fields.push({
        id,
        kind: "radio",
        label: question || options.join(" / "),
        required: shown.some((r) => isRequired(r, question)),
        options,
        value: checked ? (checked.dataset.cpLabel ?? "") : "",
      });
      continue;
    }

    const shown =
      isVisible(el) ||
      ((type === "file" || type === "checkbox") &&
        (isVisible(el.closest("label")) || isVisible(el.parentElement)));
    if (!shown) continue;

    const id = el.dataset.cp || nextId("f");
    el.dataset.cp = id;
    let label = labelOf(el);
    if (!label && type === "file") label = headingBefore(el);

    if (type === "checkbox") {
      const fieldset = el.closest("fieldset");
      const many = fieldset ? fieldset.querySelectorAll('input[type="checkbox"]').length > 1 : false;
      const question = fieldset ? questionOf([el]) : "";
      if (many && question) label = `${question}: ${label}`;
      fields.push({
        id,
        kind: "checkbox",
        label,
        required: isRequired(el, label),
        options: ["Yes", "No"],
        value: input.checked ? "Yes" : "No",
      });
      continue;
    }

    if (el.tagName === "SELECT") {
      const select = el as HTMLSelectElement;
      const options = Array.from(select.options)
        .filter((o) => o.value !== "" && !o.disabled)
        .map((o) => clean(o.textContent));
      const selected = select.selectedOptions[0];
      fields.push({
        id,
        kind: "select",
        label,
        required: isRequired(el, label),
        options,
        value: selected && selected.value !== "" ? clean(selected.textContent) : "",
      });
      continue;
    }

    const kind =
      el.tagName === "TEXTAREA"
        ? "textarea"
        : ["email", "tel", "number", "date", "url", "file"].includes(type)
          ? (type as "email" | "tel" | "number" | "date" | "url" | "file")
          : "text";
    fields.push({
      id,
      kind,
      label,
      required: isRequired(el, label),
      options: [],
      value: kind === "file" ? (input.files?.[0]?.name ?? "") : clean(input.value),
    });
  }

  // "Upload" buttons that open a file picker. SEEK has no visible file box,
  // only an Upload link beside "Resumé", so each one is reported as a file
  // field, labelled with the heading just before it.
  const uploadButtons = new Set<Element>();
  for (const el of Array.from(document.querySelectorAll<HTMLElement>('button, a, [role="button"]'))) {
    if (!isVisible(el) || (el as HTMLButtonElement).disabled) continue;
    const text = clean(el.innerText || el.getAttribute("aria-label"));
    if (!/^upload\b/i.test(text) || text.length > 40) continue;
    const label = headingBefore(el);
    uploadButtons.add(el);
    const id = el.dataset.cp || nextId("f");
    el.dataset.cp = id;
    fields.push({ id, kind: "file", label: label || text, required: false, options: [], value: "" });
  }

  // Links that act like buttons on login pages: "Create account", "Sign in".
  const authLink = /^(create (an |your )?account|sign ?up|register|sign ?in|log ?in|new user)\b/i;
  // Links that open the application form from a job ad ("Apply now").
  const startLink = /^(apply( now| online| here| for (this|the) (job|role|position))?|start (my |your |an |the )?application|begin (your |the )?application|i'?m interested)$/i;
  const buttons: PageSnapshot["buttons"] = [];
  for (const el of Array.from(
    document.querySelectorAll<HTMLElement>(
      'button, input[type="submit"], input[type="button"], a[role="button"], a[href]',
    ),
  )) {
    if (!isVisible(el) || (el as HTMLButtonElement).disabled || uploadButtons.has(el)) continue;
    const linkText = clean(el.innerText);
    if (el.tagName === "A" && el.getAttribute("role") !== "button" && !authLink.test(linkText) && !startLink.test(linkText)) {
      continue;
    }
    const text = clean(
      el.innerText || (el as HTMLInputElement).value || el.getAttribute("aria-label"),
    );
    if (!text) continue;
    const id = el.dataset.cpBtn || nextId("b");
    el.dataset.cpBtn = id;
    buttons.push({ id, text, type: (el.getAttribute("type") || "").toLowerCase() });
  }

  const errors = Array.from(
    document.querySelectorAll('[role="alert"], [aria-live="assertive"], .error, .field-error'),
  )
    .filter(isVisible)
    .map((e) => clean(e.textContent))
    .filter(Boolean)
    .slice(0, 5);

  const captchaPattern = /recaptcha|hcaptcha|challenges\.cloudflare\.com|turnstile|arkoselabs|funcaptcha/i;
  const bigEnough = (e: Element) => {
    const r = e.getBoundingClientRect();
    return isVisible(e) && r.width >= 50 && r.height >= 50;
  };
  const captcha =
    Array.from(document.querySelectorAll("iframe")).some(
      (f) => captchaPattern.test(f.src) && !/size=invisible/.test(f.src) && bigEnough(f),
    ) ||
    Array.from(document.querySelectorAll(".g-recaptcha, .h-captcha, .cf-turnstile")).some(
      (e) => e.getAttribute("data-size") !== "invisible" && bigEnough(e),
    );

  const passwordBoxes = Array.from(
    document.querySelectorAll<HTMLInputElement>('input[type="password"]'),
  ).filter((el) => isVisible(el) && !el.disabled);
  const password = passwordBoxes.length > 0;
  const secrets = passwordBoxes.map((el) => {
    const id = el.dataset.cp || nextId("f");
    el.dataset.cp = id;
    return id;
  });

  const headings = Array.from(document.querySelectorAll("h1, h2, h3"))
    .filter(isVisible)
    .map((h) => clean(h.textContent))
    .filter(Boolean)
    .slice(0, 6);

  return {
    url: location.href,
    title: document.title,
    headings,
    fields,
    buttons,
    errors,
    captcha,
    password,
    secrets,
    text: (document.body?.innerText ?? "").replace(/\s+/g, " ").trim().slice(0, 2000),
  };
}

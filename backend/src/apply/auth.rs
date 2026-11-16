//! Telling login, sign-up and email-check pages apart from application forms.
//!
//! Employer careers sites (Workday and the like) want an account before you
//! can apply: sign in, or create an account, then type a code they email you
//! or click a link in that email. These are pure functions over the page
//! snapshot, so they're easy to test; the runner does the clicking.

use super::browser::{Button, ButtonKind, Field, FieldKind, Snapshot};
use super::fill::normalise;

#[derive(Debug)]
pub enum Page<'a> {
    /// An ordinary form page: nothing to do with logging in.
    Form,
    /// One password box: sign in. `create` is a "Create account" link, if any.
    Login {
        user: Option<&'a Field>,
        password: &'a str,
        button: Option<&'a Button>,
        create: Option<&'a Button>,
    },
    /// Two password boxes (or "Create account" around one): sign up.
    SignUp {
        user: Option<&'a Field>,
        passwords: &'a [String],
        button: Option<&'a Button>,
    },
    /// Only an email box: "Sign in with a code" sites, or the first step of a
    /// two-step sign-in (email, then password on the next page).
    EmailLogin {
        user: &'a Field,
        button: Option<&'a Button>,
    },
    /// "Enter the code we emailed you".
    Code {
        field: &'a Field,
        button: Option<&'a Button>,
    },
    /// "Check your email and click the link to verify your account".
    CheckEmail,
}

const SIGN_UP: [&str; 5] = [
    "create account",
    "create an account",
    "create your account",
    "sign up",
    "register",
];
const SIGN_IN: [&str; 3] = ["sign in", "log in", "login"];
/// Buttons that email you a sign-in code.
const SEND_CODE: [&str; 6] = [
    "email me",
    "send code",
    "send me a code",
    "send a code",
    "get code",
    "continue with email",
];

fn any_of(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| text.contains(w))
}

/// "Create account", "Sign up", "Register" links and buttons.
fn create_button(buttons: &[Button]) -> Option<&Button> {
    buttons
        .iter()
        .find(|b| any_of(&normalise(&b.text), &SIGN_UP) || normalise(&b.text) == "new user")
}

/// The button that sends this login, sign-up or code page: the first one
/// that isn't Back or a link to somewhere else.
fn send_button<'a>(buttons: &'a [Button], words: &[&str]) -> Option<&'a Button> {
    buttons
        .iter()
        .find(|b| any_of(&normalise(&b.text), words))
        .or_else(|| {
            buttons
                .iter()
                .find(|b| matches!(b.kind, ButtonKind::Submit | ButtonKind::Next))
        })
}

/// The username or email box on a login or sign-up page.
fn user_field(fields: &[Field]) -> Option<&Field> {
    fields.iter().find(|f| {
        let label = normalise(&f.label);
        f.kind == FieldKind::Email
            || label.contains("email")
            || label.contains("username")
            || label.contains("user name")
    })
}

/// "Verification code", "One-time code", "Enter the code".
pub fn is_code_field(field: &Field) -> bool {
    let label = normalise(&field.label);
    matches!(
        field.kind,
        FieldKind::Text | FieldKind::Number | FieldKind::Tel
    ) && (label.contains("code") || label.contains("otp") || label.contains("one time"))
        && !label.contains("post")
        && !label.contains("zip")
        && !label.contains("promo")
        && !label.contains("referral")
        && !label.contains("area")
}

pub fn classify(snap: &Snapshot) -> Page<'_> {
    let heading = normalise(&snap.headings.join(" "));
    let text = normalise(&snap.text);

    if let Some(field) = snap
        .fields
        .iter()
        .find(|f| f.is_empty() && is_code_field(f))
    {
        return Page::Code {
            field,
            button: send_button(
                &snap.buttons,
                &[
                    "verify", "confirm", "continue", "submit", "sign in", "log in", "next",
                ],
            ),
        };
    }

    match snap.secrets.len() {
        0 => {
            let check_email = [
                "verify your email",
                "confirm your email",
                "check your email",
                "check your inbox",
                "activate your account",
                "verification link",
            ];
            let inputs: Vec<&Field> = snap
                .fields
                .iter()
                .filter(|f| f.kind != FieldKind::Checkbox)
                .collect();
            let code_button = snap
                .buttons
                .iter()
                .any(|b| any_of(&normalise(&b.text), &SEND_CODE));
            if let [only] = inputs.as_slice()
                && user_field(&snap.fields).is_some_and(|u| u.id == only.id)
                && (any_of(&heading, &SIGN_IN) || code_button)
            {
                let mut words: Vec<&str> = SEND_CODE.to_vec();
                words.extend(["continue", "next"]);
                words.extend(SIGN_IN);
                return Page::EmailLogin {
                    user: only,
                    button: send_button(&snap.buttons, &words),
                };
            }
            let has_inputs = !inputs.is_empty();
            if !has_inputs && (any_of(&heading, &check_email) || any_of(&text, &check_email)) {
                Page::CheckEmail
            } else {
                Page::Form
            }
        }
        1 if any_of(&heading, &SIGN_UP) && !any_of(&heading, &SIGN_IN) => Page::SignUp {
            user: user_field(&snap.fields),
            passwords: &snap.secrets,
            button: send_button(&snap.buttons, &SIGN_UP),
        },
        1 => Page::Login {
            user: user_field(&snap.fields),
            password: &snap.secrets[0],
            button: send_button(&snap.buttons, &SIGN_IN),
            create: create_button(&snap.buttons),
        },
        _ => Page::SignUp {
            user: user_field(&snap.fields),
            passwords: &snap.secrets,
            button: send_button(&snap.buttons, &SIGN_UP),
        },
    }
}

/// "I agree to the Terms of Use and Privacy Policy": ticked for you once you
/// said yes to creating the account.
pub fn is_terms(field: &Field) -> bool {
    let label = normalise(&field.label);
    field.kind == FieldKind::Checkbox
        && (label.contains("terms") || label.contains("privacy") || label.contains("agree"))
}

/// The site a page belongs to, for the vault: "acme.wd3.myworkdayjobs.com".
pub fn site_of(url: &str) -> Option<String> {
    crate::vault::normalize_site(url)
}

/// A short word to find this site's emails by: "myworkdayjobs" for
/// acme.wd3.myworkdayjobs.com, "practice" for 127.0.0.1.
pub fn email_hint(site: &str, company: Option<&str>) -> String {
    if let Some(company) = company.map(str::trim).filter(|c| !c.is_empty()) {
        return company
            .split_whitespace()
            .next()
            .unwrap_or(company)
            .to_lowercase();
    }
    let parts: Vec<&str> = site.split('.').collect();
    if parts.len() >= 2 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
        return "practice".into();
    }
    parts
        .iter()
        .rev()
        .find(|p| !matches!(**p, "com" | "au" | "net" | "org" | "io" | "co" | "jobs"))
        .unwrap_or(&"")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: &str, kind: FieldKind, label: &str) -> Field {
        let checkbox = kind == FieldKind::Checkbox;
        Field {
            id: id.into(),
            kind,
            label: label.into(),
            required: true,
            options: if checkbox {
                vec!["Yes".into(), "No".into()]
            } else {
                vec![]
            },
            value: if checkbox { "No".into() } else { String::new() },
        }
    }

    fn button(id: &str, text: &str, kind: ButtonKind) -> Button {
        Button {
            id: id.into(),
            text: text.into(),
            kind,
        }
    }

    fn snap(
        headings: &[&str],
        fields: Vec<Field>,
        buttons: Vec<Button>,
        secrets: &[&str],
    ) -> Snapshot {
        Snapshot {
            url: "https://acme.wd3.myworkdayjobs.com/en-US/careers/login".into(),
            title: String::new(),
            headings: headings.iter().map(|h| h.to_string()).collect(),
            fields,
            buttons,
            errors: vec![],
            captcha: false,
            password: !secrets.is_empty(),
            secrets: secrets.iter().map(|s| s.to_string()).collect(),
            text: headings.join(" "),
        }
    }

    #[test]
    fn a_login_page_with_a_create_account_link() {
        let s = snap(
            &["Sign in to apply"],
            vec![field("f1", FieldKind::Email, "Email address")],
            vec![
                button("b1", "Sign in", ButtonKind::Other),
                button("b2", "Create an account", ButtonKind::Other),
            ],
            &["f2"],
        );
        let Page::Login {
            user,
            password,
            button,
            create,
        } = classify(&s)
        else {
            panic!("not a login page");
        };
        assert_eq!(user.unwrap().id, "f1");
        assert_eq!(password, "f2");
        assert_eq!(button.unwrap().id, "b1");
        assert_eq!(create.unwrap().id, "b2");
    }

    #[test]
    fn two_password_boxes_mean_sign_up() {
        let s = snap(
            &["Create your account"],
            vec![
                field("f1", FieldKind::Text, "First name *"),
                field("f2", FieldKind::Email, "Email address *"),
                field("f5", FieldKind::Checkbox, "I agree to the Terms of Use *"),
            ],
            vec![button("b1", "Create account", ButtonKind::Other)],
            &["f3", "f4"],
        );
        let Page::SignUp {
            user,
            passwords,
            button,
        } = classify(&s)
        else {
            panic!("not a sign-up page");
        };
        assert_eq!(user.unwrap().id, "f2");
        assert_eq!(passwords, ["f3", "f4"]);
        assert_eq!(button.unwrap().id, "b1");
        assert!(is_terms(&s.fields[2]));
        assert!(!is_terms(&s.fields[0]));
    }

    #[test]
    fn code_pages_and_check_your_email_pages() {
        let s = snap(
            &["Verify your email"],
            vec![field("f1", FieldKind::Text, "Verification code")],
            vec![button("b1", "Verify", ButtonKind::Other)],
            &[],
        );
        assert!(matches!(classify(&s), Page::Code { field, .. } if field.id == "f1"));

        // "Sign in" sends the code on code-only sign-in pages.
        let s = snap(
            &["Check your email"],
            vec![field("f1", FieldKind::Text, "Sign-in code")],
            vec![
                button("b0", "Back", ButtonKind::Back),
                button("b1", "Sign in", ButtonKind::Other),
            ],
            &[],
        );
        assert!(matches!(
            classify(&s),
            Page::Code { button: Some(b), .. } if b.id == "b1"
        ));

        let s = snap(
            &["Check your email to activate your account"],
            vec![],
            vec![],
            &[],
        );
        assert!(matches!(classify(&s), Page::CheckEmail));

        // A postcode is not a verification code, and a form is a form.
        let s = snap(
            &["Your details"],
            vec![field("f1", FieldKind::Text, "Postcode")],
            vec![button("b1", "Continue", ButtonKind::Next)],
            &[],
        );
        assert!(matches!(classify(&s), Page::Form));
    }

    #[test]
    fn email_only_sign_in() {
        // "Sign in with a code": just an email box.
        let s = snap(
            &["Sign in"],
            vec![field("f1", FieldKind::Email, "Email address")],
            vec![
                button("b0", "Back", ButtonKind::Back),
                button("b1", "Email me a sign-in code", ButtonKind::Submit),
            ],
            &[],
        );
        assert!(matches!(
            classify(&s),
            Page::EmailLogin { user, button: Some(b) } if user.id == "f1" && b.id == "b1"
        ));

        // An application step that asks for an email among other things is a form.
        let s = snap(
            &["Sign in or apply"],
            vec![
                field("f1", FieldKind::Email, "Email address"),
                field("f2", FieldKind::Tel, "Mobile"),
            ],
            vec![button("b1", "Continue", ButtonKind::Next)],
            &[],
        );
        assert!(matches!(classify(&s), Page::Form));

        // Just an email box, but nothing says sign in: a form.
        let s = snap(
            &["Stay in touch"],
            vec![field("f1", FieldKind::Email, "Email address")],
            vec![button("b1", "Continue", ButtonKind::Next)],
            &[],
        );
        assert!(matches!(classify(&s), Page::Form));
    }

    #[test]
    fn email_hints() {
        assert_eq!(
            email_hint("acme.wd3.myworkdayjobs.com", Some("Acme Bank")),
            "acme"
        );
        assert_eq!(email_hint("careers.acme.com.au", None), "acme");
        assert_eq!(email_hint("127.0.0.1", None), "practice");
    }
}

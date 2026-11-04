//! Korean when the system's first language is Korean, English otherwise. The window asks the
//! same through the `lang` command, so menus, messages and the page agree.

use std::sync::OnceLock;

static IS_KO: OnceLock<bool> = OnceLock::new();

pub fn is_ko() -> bool {
    // Tests check the Korean messages.
    *IS_KO.get_or_init(|| cfg!(test) || sys_locale::get_locale().is_some_and(|l| l.starts_with("ko")))
}

/// `t("취소됨", "Cancelled")`
pub fn t(ko: &'static str, en: &'static str) -> &'static str {
    if is_ko() { ko } else { en }
}

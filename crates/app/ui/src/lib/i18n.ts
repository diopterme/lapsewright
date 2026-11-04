// crates/app/src/lang.rs decides; main.ts sets it before any other module loads, so
// module-level constants may call `t` too. Node tests get Korean.
let isKo = true;

export function setLang(lang: string) {
  isKo = lang === "ko";
  document.documentElement.lang = lang;
}

/** `t("취소", "Cancel")` */
export const t = (ko: string, en: string) => (isKo ? ko : en);

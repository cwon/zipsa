import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const storageKey = "zipsa.theme";
const darkQuery = "(prefers-color-scheme: dark)";

export function readThemePreference(): ThemePreference {
  try {
    const value = localStorage.getItem(storageKey);
    if (value === "light" || value === "dark") return value;
  } catch {
    // 저장소를 사용할 수 없으면 시스템 설정을 따른다.
  }
  return "system";
}

function storeThemePreference(preference: ThemePreference) {
  try {
    if (preference === "system") localStorage.removeItem(storageKey);
    else localStorage.setItem(storageKey, preference);
  } catch {
    // 저장하지 못해도 현재 실행 중인 화면에는 적용한다.
  }
}

/** 화면 색상과 네이티브 창(제목 표시줄)의 테마를 함께 바꾼다. */
export function applyTheme(preference: ThemePreference) {
  const root = document.documentElement;
  // 테마 전환 순간에는 색상 전환 애니메이션이 요소마다 어긋나 보이지 않도록 끈다.
  root.classList.add("no-theme-transition");
  if (preference === "system") delete root.dataset.theme;
  else root.dataset.theme = preference;
  void root.offsetWidth;
  root.classList.remove("no-theme-transition");

  if (!isTauri()) return;
  try {
    void getCurrentWindow()
      .setTheme(preference === "system" ? null : preference)
      .catch(() => {});
  } catch {
    // 창 정보를 얻지 못한 환경에서는 화면 색상만 바꾼다.
  }
}

export function useTheme() {
  const [preference, setPreferenceState] =
    useState<ThemePreference>(readThemePreference);
  const [systemDark, setSystemDark] = useState(
    () => window.matchMedia(darkQuery).matches,
  );

  useEffect(() => {
    const media = window.matchMedia(darkQuery);
    const onChange = () => setSystemDark(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  function setPreference(next: ThemePreference) {
    setPreferenceState(next);
    storeThemePreference(next);
    applyTheme(next);
  }

  const resolved: ResolvedTheme =
    preference === "system" ? (systemDark ? "dark" : "light") : preference;
  return { preference, resolved, setPreference };
}

import { test, expect, type Page } from "@playwright/test";
import type { ReleaseCheck, Snapshot, Task } from "../../src/types";

const empty: Snapshot = {
  spaces: [],
  items: [],
  tasks: [],
  records: [],
  settings: { notifications: false },
};
const home = {
  id: "home",
  parentId: null,
  name: "우리 집",
  icon: "🏠",
  notes: "나의 편안한 공간",
};

async function mockDesktop(
  page: Page,
  initial: Snapshot = empty,
  update: { result?: ReleaseCheck; error?: string } = {},
) {
  await page.addInitScript(
    ({ initial, update }) => {
      const state = structuredClone(initial);
      const nativeThemes: (string | null)[] = [];
      const updateRequests = { checks: 0, pages: [] as boolean[] };
      Object.assign(window, {
        isTauri: true,
        __nativeThemes: nativeThemes,
        __updates: updateRequests,
        __TAURI_INTERNALS__: {
          metadata: {
            currentWindow: { label: "main" },
            currentWebview: { windowLabel: "main", label: "main" },
          },
          invoke: async (command: string, args: Record<string, any> = {}) => {
            if (command === "load_data") return structuredClone(state);
            if (command === "app_info")
              return {
                version: "0.1.0",
                dataPath: "/local/Zipsa",
              };
            if (command === "plugin:autostart|is_enabled") return false;
            if (command === "plugin:window|set_theme") {
              nativeThemes.push(args.value);
              return null;
            }
            if (command === "check_latest_release") {
              updateRequests.checks += 1;
              if (update.error) throw update.error;
              return (
                update.result ?? {
                  latestVersion: null,
                  updateAvailable: false,
                  notes: null,
                }
              );
            }
            if (command === "open_download_page") {
              updateRequests.pages.push(args.latest);
              return null;
            }
            if (command.startsWith("save_") && command !== "save_settings") {
              const kind = command.slice(5);
              const value = {
                ...args[kind],
                id: args[kind].id || crypto.randomUUID(),
              };
              const key = `${kind}s` as "spaces" | "items" | "tasks";
              (state[key] as any[]) = (state[key] as any[])
                .filter((v) => v.id !== value.id)
                .concat(value);
              return structuredClone(state);
            }
            if (command === "finish_task") {
              const task = state.tasks.find((t) => t.id === args.id)!;
              task.active = false;
              state.records.push({
                id: crypto.randomUUID(),
                taskId: task.id,
                title: task.title,
                target: "우리 집 / 우유",
                scheduledFor: task.dueAt,
                completedAt: new Date().toISOString(),
                outcome: args.outcome,
                taskAfter: structuredClone(task),
              });
              return structuredClone(state);
            }
            if (command === "reopen_task") {
              const record = state.records.find((r) => r.id === args.recordId)!;
              const task = state.tasks.find((t) => t.id === args.id)!;
              task.active = true;
              task.dueAt = record.scheduledFor;
              task.notifiedDueAt = null;
              state.records = state.records.filter((r) => r.id !== record.id);
              return structuredClone(state);
            }
            if (command === "delete_entity" && args.kind === "task") {
              state.tasks = state.tasks.filter((t) => t.id !== args.id);
              return structuredClone(state);
            }
            if (command === "delete_entity")
              throw "하위 공간·아이템·할 일을 먼저 옮기거나 삭제해 주세요.";
            if (command === "plugin:notification|is_permission_granted")
              return false;
            if (command === "plugin:notification|request_permission")
              return "denied";
            throw new Error(`Unexpected IPC: ${command}`);
          },
        },
      });
    },
    { initial, update },
  );
}

test("creates nested spaces and finds them with global search", async ({
  page,
}, info) => {
  await mockDesktop(page);
  await page.goto("/");
  await page.getByRole("button", { name: "첫 공간 만들기" }).click();
  await page.getByLabel("공간 이름").fill("우리 집");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await page
    .getByRole("navigation", { name: "공간 트리" })
    .getByRole("button", { name: "우리 집" })
    .click();
  await page.getByRole("button", { name: "하위 공간 추가" }).click();
  await page.getByLabel("공간 이름").fill("책장");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "책장", exact: true }),
  ).toBeVisible();
  await page.getByLabel("공간, 아이템, 할 일 검색").fill("책장");
  await expect(
    page.getByRole("heading", { name: "‘책장’ 검색 결과" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "책장", exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: info.outputPath("search.png") });
});

test("saves a photo and purchase details, binds an item task, and shows history", async ({
  page,
}) => {
  await mockDesktop(page, { ...empty, spaces: [home] });
  await page.goto("/");
  await page.getByRole("button", { name: "모든 아이템", exact: true }).click();
  await page
    .getByRole("button", { name: "아이템 추가", exact: true })
    .first()
    .click();
  await page.getByLabel("아이템 이름").fill("우유");
  await page.getByLabel("수량").fill("2");
  await page.getByLabel("구매처").fill("동네 마트");
  await page.getByLabel("구매 금액 (원)").fill("4800");
  await page.getByLabel("유통기한").fill("2099-12-31");
  await page.locator("input[type=file]").setInputFiles({
    name: "milk.png",
    mimeType: "image/png",
    buffer: Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jp1sAAAAASUVORK5CYII=",
      "base64",
    ),
  });
  await expect(
    page.getByRole("button", { name: "저장", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await expect(
    page.getByRole("img", { name: "우유", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("동네 마트 · 4,800원")).toBeVisible();
  await page.getByRole("button", { name: "할 일 연결" }).click();
  await expect(page.getByLabel("연결 대상")).toHaveValue("item");
  await page.getByLabel("할 일 이름").fill("우유 확인하기");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await page
    .getByRole("navigation", { name: "주 메뉴" })
    .getByRole("button", { name: /^할 일/ })
    .click();
  await expect(page.getByText("우리 집 / 우유", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { name: "우유 확인하기 완료", exact: true })
    .click();
  await page.getByRole("button", { name: "수행 기록", exact: true }).click();
  await expect(page.getByText("우유 확인하기", { exact: true })).toBeVisible();
  await expect(page.getByText("완료", { exact: true })).toBeVisible();
});

test("keeps notification disabled when OS permission is denied", async ({
  page,
}) => {
  await mockDesktop(page);
  await page.goto("/");
  await page.getByRole("button", { name: /^설정/ }).click();
  await page.getByRole("switch", { name: "할 일 알림", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "시스템 설정에서 집사의 알림을 허용",
  );
  await expect(
    page.getByRole("switch", { name: "할 일 알림", exact: true }),
  ).toHaveAttribute("aria-checked", "false");
});

test("shows deletion errors inside the active confirmation dialog", async ({
  page,
}) => {
  await mockDesktop(page, { ...empty, spaces: [home] });
  await page.goto("/");
  await page
    .getByRole("navigation", { name: "공간 트리" })
    .getByRole("button", { name: "우리 집" })
    .click();
  await page.getByRole("button", { name: "공간 수정" }).click();
  await page.getByRole("button", { name: "삭제", exact: true }).click();
  const dialog = page.getByRole("dialog").last();
  await dialog.getByRole("button", { name: "삭제", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("먼저 옮기거나 삭제");
  await expect(dialog).toBeVisible();
});

test("switches between light and dark themes and remembers the choice", async ({
  page,
}) => {
  const root = page.locator("html");
  const body = page.locator("body");
  const lastNativeTheme = () =>
    page.evaluate(() =>
      (window as unknown as { __nativeThemes: unknown[] }).__nativeThemes.at(
        -1,
      ),
    );
  await page.emulateMedia({ colorScheme: "light" });
  await mockDesktop(page, { ...empty, spaces: [home] });
  await page.goto("/");
  await expect(root).not.toHaveAttribute("data-theme");
  await expect(body).toHaveCSS("background-color", "rgb(255, 255, 255)");

  await page.getByRole("button", { name: /^설정/ }).click();
  const dark = page.getByRole("radio", { name: "다크", exact: true });
  await dark.click();
  await expect(dark).toHaveAttribute("aria-checked", "true");
  await expect(root).toHaveAttribute("data-theme", "dark");
  await expect(body).toHaveCSS("background-color", "rgb(17, 17, 17)");
  await expect.poll(lastNativeTheme).toBe("dark");

  await page.reload();
  await expect(root).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: "라이트 테마로 전환" }).click();
  await expect(root).toHaveAttribute("data-theme", "light");
  await expect(body).toHaveCSS("background-color", "rgb(255, 255, 255)");
  await expect.poll(lastNativeTheme).toBe("light");

  await page.getByRole("button", { name: /^설정/ }).click();
  await page.getByRole("radio", { name: "시스템 설정", exact: true }).click();
  await expect(root).not.toHaveAttribute("data-theme");
  await expect.poll(lastNativeTheme).toBe(null);
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(body).toHaveCSS("background-color", "rgb(17, 17, 17)");
  await expect(
    page.getByRole("button", { name: "라이트 테마로 전환" }),
  ).toBeVisible();
});

test("explains inside the space dialog why a space is needed first", async ({
  page,
}) => {
  await mockDesktop(page);
  await page.goto("/");
  const dialog = page.getByRole("dialog");
  const spaceName = dialog.getByLabel("공간 이름");
  const taskReason = /할 일은 공간이나 아이템에 연결해야 저장할 수 있어요/;
  const mainNav = page.getByRole("navigation", { name: "주 메뉴" });

  await page.getByRole("button", { name: "할 일 추가", exact: true }).click();
  await expect(
    dialog.getByRole("heading", { name: "공간 추가" }),
  ).toBeVisible();
  await expect(spaceName).toHaveAccessibleDescription(taskReason);
  await dialog.getByRole("button", { name: "취소" }).click();

  await mainNav.getByRole("button", { name: "모든 아이템" }).click();
  await page
    .getByRole("button", { name: "아이템 추가", exact: true })
    .first()
    .click();
  await expect(spaceName).toHaveAccessibleDescription(
    /아이템은 보관할 공간을 정해야 저장할 수 있어요/,
  );
  await dialog.getByRole("button", { name: "취소" }).click();

  await mainNav.getByRole("button", { name: /^할 일/ }).click();
  await page.getByRole("button", { name: "할 일 추가", exact: true }).click();
  await expect(spaceName).toHaveAccessibleDescription(taskReason);
  await dialog.getByRole("button", { name: "취소" }).click();

  await mainNav.getByRole("button", { name: "오늘의 집사" }).click();
  await page.getByRole("button", { name: "첫 공간 만들기" }).click();
  await expect(spaceName).toBeVisible();
  await expect(dialog.getByText("먼저 공간을 만들어 주세요")).toHaveCount(0);
});

const cleaning: Task = {
  id: "cleaning",
  spaceId: "home",
  itemId: null,
  title: "냉장고 청소",
  notes: "",
  mode: "once",
  every: 1,
  unit: "month",
  anchorAt: "2026-11-07T00:00:00Z",
  dueAt: "2026-11-07T00:00:00Z",
  timezone: "Asia/Seoul",
  active: true,
  notifiedDueAt: null,
};

test("reopens a completed task from its check button and deletes it while retaining history", async ({
  page,
}) => {
  await mockDesktop(page, { ...empty, spaces: [home], tasks: [cleaning] });
  await page.goto("/");
  const nav = page.getByRole("navigation", { name: "주 메뉴" });
  await nav.getByRole("button", { name: /^할 일/ }).click();
  await page
    .getByRole("button", { name: "냉장고 청소 완료", exact: true })
    .click();
  await page.getByRole("button", { name: "종료된 일", exact: true }).click();
  await page
    .getByRole("button", { name: "냉장고 청소 미완료로 되돌리기", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "냉장고 청소 완료", exact: true }),
  ).toBeVisible();
  await nav.getByRole("button", { name: "수행 기록", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "돌본 흔적을 모아 둘게요" }),
  ).toBeVisible();
  await nav.getByRole("button", { name: /^할 일/ }).click();
  await page
    .getByRole("button", { name: "냉장고 청소 완료", exact: true })
    .click();
  await page.getByRole("button", { name: "종료된 일", exact: true }).click();
  await page
    .getByRole("button", { name: "냉장고 청소 삭제", exact: true })
    .click();
  let dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("heading", { name: "할 일을 삭제할까요?" }),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "취소", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "냉장고 청소 삭제", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "냉장고 청소 삭제", exact: true })
    .click();
  dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "삭제", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "냉장고 청소 삭제", exact: true }),
  ).toHaveCount(0);
  await nav.getByRole("button", { name: "수행 기록", exact: true }).click();
  await expect(page.getByText("냉장고 청소", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", {
      name: "냉장고 청소 미완료로 되돌리기",
      exact: true,
    }),
  ).toHaveCount(0);
});

test("undoes the latest recurring occurrence from history and restores its due date", async ({
  page,
}) => {
  const task: Task = {
    ...cleaning,
    mode: "calendar",
    dueAt: "2027-01-07T00:00:00Z",
  };
  const first = {
    id: "first",
    taskId: task.id,
    title: task.title,
    target: "우리 집",
    scheduledFor: "2026-11-07T00:00:00Z",
    completedAt: "2026-11-07T01:00:00Z",
    outcome: "completed" as const,
  };
  const last = {
    ...first,
    id: "last",
    scheduledFor: "2026-12-07T00:00:00Z",
    completedAt: "2026-12-07T01:00:00Z",
  };
  await mockDesktop(page, {
    ...empty,
    spaces: [home],
    tasks: [task],
    records: [first, last],
  });
  await page.goto("/");
  const nav = page.getByRole("navigation", { name: "주 메뉴" });
  await nav.getByRole("button", { name: "수행 기록", exact: true }).click();
  await expect(page.locator(".history-row")).toHaveCount(2);
  await expect(
    page.locator(".history-row").last().getByRole("button"),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "냉장고 청소 미완료로 되돌리기", exact: true })
    .click();
  await expect(page.locator(".history-row")).toHaveCount(1);
  await nav.getByRole("button", { name: /^할 일/ }).click();
  await page.locator(".task-body").click();
  await expect(page.getByLabel(/^다음 예정일/)).toHaveValue("2026-12-07T09:00");
});

test("deletes a pending task directly from the task list", async ({ page }) => {
  await mockDesktop(page, { ...empty, spaces: [home], tasks: [cleaning] });
  await page.goto("/");
  await page
    .getByRole("navigation", { name: "주 메뉴" })
    .getByRole("button", { name: /^할 일/ })
    .click();
  await page
    .getByRole("button", { name: "냉장고 청소 삭제", exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "삭제", exact: true })
    .click();
  await expect(
    page.getByText("등록된 할 일이 없어요.", { exact: true }),
  ).toBeVisible();
});

async function updateRequests(page: Page) {
  return page.evaluate(
    () =>
      (window as unknown as { __updates: { checks: number; pages: boolean[] } })
        .__updates,
  );
}

test("checks on demand and opens the new release page only after the download button is clicked", async ({
  page,
}) => {
  await mockDesktop(page, empty, {
    result: {
      latestVersion: "0.2.0",
      updateAvailable: true,
      notes: "공간 검색 개선\n사진 관리 개선",
    },
  });
  await page.goto("/");
  await page.getByRole("button", { name: /^설정/ }).click();
  expect(await updateRequests(page)).toEqual({ checks: 0, pages: [] });
  await page
    .getByRole("button", { name: "최신 버전 확인", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "v0.2.0 새 버전이 있어요.",
  );
  await expect(page.locator(".release-notes")).toContainText("공간 검색 개선");
  expect(await updateRequests(page)).toEqual({ checks: 1, pages: [] });
  await expect(
    page.getByRole("button", { name: /설치|다시 시작/ }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "다운로드 페이지로 이동", exact: true })
    .click();
  expect(await updateRequests(page)).toEqual({ checks: 1, pages: [true] });
});

test("does not offer an update when the installed version is current", async ({
  page,
}) => {
  await mockDesktop(page, empty, {
    result: { latestVersion: "0.1.0", updateAvailable: false, notes: null },
  });
  await page.goto("/");
  await page.getByRole("button", { name: /^설정/ }).click();
  await page
    .getByRole("button", { name: "최신 버전 확인", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText("새로운 버전이 없어요.");
  await expect(
    page.getByRole("button", { name: "다운로드 페이지로 이동", exact: true }),
  ).toHaveCount(0);
});

test("explains that no release has been published and still offers the release list", async ({
  page,
}) => {
  await mockDesktop(page);
  await page.goto("/");
  await page.getByRole("button", { name: /^설정/ }).click();
  await page
    .getByRole("button", { name: "최신 버전 확인", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "아직 공개된 정식 릴리스가 없어요.",
  );
  await page
    .getByRole("button", { name: "GitHub 릴리스 보기", exact: true })
    .click();
  expect((await updateRequests(page)).pages).toEqual([false]);
});

test("allows retry and direct release-page navigation after a failed version check", async ({
  page,
}) => {
  await mockDesktop(page, empty, { error: "GitHub 요청이 제한되었어요." });
  await page.goto("/");
  await page.getByRole("button", { name: /^설정/ }).click();
  const button = page.getByRole("button", {
    name: "최신 버전 확인",
    exact: true,
  });
  await button.click();
  await expect(page.getByRole("alert")).toContainText(
    "GitHub 요청이 제한되었어요.",
  );
  await expect(button).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "다운로드 페이지로 이동", exact: true }),
  ).toHaveCount(0);
  await button.click();
  await expect.poll(async () => (await updateRequests(page)).checks).toBe(2);
  await page
    .getByRole("button", { name: "GitHub 릴리스 보기", exact: true })
    .click();
  expect((await updateRequests(page)).pages).toEqual([false]);
});

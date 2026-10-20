import { test, expect } from "@playwright/test";
import path from "node:path";

test("onboarding renders the local-first setup flow", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("让每一个故事")).toBeVisible();
  await expect(page.getByText("模型连接")).toBeVisible();
  await expect(page.getByRole("button", { name: "创建项目并进入" })).toBeVisible();
  await page.screenshot({
    path: path.join("test-results", "onboarding.png"),
    fullPage: true,
  });
});

import { describe, expect, it } from "vitest";
import {
  BUILD_APP_VERSION,
  classifyUpdateFailure,
  describeUpdateFailureWithRaw,
  resolveAppVersion,
} from "./utils-update";
import type { UpdaterConfigInfo } from "./types";

const config: UpdaterConfigInfo = {
  endpoints: ["https://github.com/RaInSLc/R_links/releases/latest/download/latest.json"],
  pubkeyKeyId: "4D8250359656C71F",
  currentVersion: "0.2.5",
};

describe("resolveAppVersion", () => {
  it("优先使用运行时版本号", () => {
    expect(resolveAppVersion("9.9.9")).toBe("9.9.9");
  });

  it("运行时版本号缺失或为空白时回退到构建期版本", () => {
    expect(resolveAppVersion("")).toBe(BUILD_APP_VERSION);
    expect(resolveAppVersion("   ")).toBe(BUILD_APP_VERSION);
  });
});

describe("classifyUpdateFailure", () => {
  it("把「清单缺失」与「网络不可达」区分开", () => {
    // 这是当前真实症状：GitHub Release 里没有 latest.json，端点返回 404 的 HTML 页面。
    expect(classifyUpdateFailure(new Error("Could not fetch a valid release JSON from the remote")))
      .toBe("manifest-missing");
    expect(classifyUpdateFailure(new Error("error sending request for url (https://github.com/...)")))
      .toBe("network");
  });

  it("识别签名校验失败", () => {
    expect(classifyUpdateFailure(new Error("signature verification failed"))).toBe("signature");
    expect(classifyUpdateFailure(new Error("invalid minisign public key"))).toBe("signature");
  });

  it("识别权限缺失与未知错误", () => {
    expect(classifyUpdateFailure(new Error("updater.check not allowed"))).toBe("permission");
    expect(classifyUpdateFailure(new Error("something else entirely"))).toBe("unknown");
    expect(classifyUpdateFailure(undefined)).toBe("unknown");
  });

  it("清单缺失优先于网络类关键词，避免 404 被误判成断网", () => {
    expect(classifyUpdateFailure(new Error("404 Not Found while connecting to origin")))
      .toBe("manifest-missing");
  });
});

describe("describeUpdateFailureWithRaw", () => {
  it("清单缺失时给出更新端点与手动的下一步", () => {
    const message = describeUpdateFailureWithRaw({
      stage: "manifest-missing",
      rawMessage: "Could not fetch a valid release JSON from the remote",
      config,
    });
    expect(message).toContain("latest.json");
    expect(message).toContain("releases/latest/download/latest.json");
    expect(message).toContain("GitHub Releases 手动下载");
  });

  it("签名失败时带上内置公钥 ID，便于核对发布密钥", () => {
    const message = describeUpdateFailureWithRaw({
      stage: "signature",
      rawMessage: "signature verify failed",
      config,
    });
    expect(message).toContain("4D8250359656C71F");
  });

  it("公钥无法解析时指向具体配置项", () => {
    const message = describeUpdateFailureWithRaw({
      stage: "signature",
      rawMessage: "invalid encoding",
      config: { ...config, pubkeyKeyId: null },
    });
    expect(message).toContain("plugins.updater.pubkey");
  });

  it("网络失败时带上端点并保留原始错误", () => {
    const message = describeUpdateFailureWithRaw({
      stage: "network",
      rawMessage: "connection timed out",
      config,
    });
    expect(message).toContain("无法连接更新源");
    expect(message).toContain("connection timed out");
  });

  it("未知失败时直接回显原始错误", () => {
    expect(
      describeUpdateFailureWithRaw({ stage: "unknown", rawMessage: "boom", config: null }),
    ).toContain("boom");
  });
});

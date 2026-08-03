import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { generateMultiEcosystemScript, type SearchResult } from "./utils";
import type { Ecosystem, Method, Settings } from "./types";

export function useScriptGeneration(input: string, ecosystem: Ecosystem, pipIndex: string, condaChannels: string[], method: Method, conditional: boolean, installDependencies: boolean, showRemoteVersion: boolean, verifyInstall: boolean, parallelInstall: boolean, settings: Settings, rBinaryMirror: string, results: SearchResult[], inputTooLarge: boolean, setStatus: (status: string) => void) {
  const [script, setScriptState] = useState("等待输入...");
  const latestScriptRef = useRef("等待输入...");
  const requestSeq = useRef(0);

  function setScript(next: string) {
    latestScriptRef.current = next;
    setScriptState(next);
  }

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(() => {
      const seq = requestSeq.current + 1;
      requestSeq.current = seq;
      if (inputTooLarge) {
        setScript("输入超出限制，无法生成脚本。");
        return;
      }
      if (ecosystem === "pip" || ecosystem === "conda") {
        setScript(generateMultiEcosystemScript(input, ecosystem, pipIndex, condaChannels));
        return;
      }
      invoke<string>("generate_script", {
        input,
        options: { method, conditional, installDependencies, mirror: ecosystem === "r-binary" ? rBinaryMirror : settings.cranMirror, rLibPath: settings.rLibPath, archiveGithubMajorGap: settings.archiveGithubMajorGap, appendVerify: verifyInstall, parallelInstall },
        results,
        showRemoteVersion,
      }).then((next) => {
        if (active && seq === requestSeq.current) setScript(next);
      }).catch((error) => {
        if (active && seq === requestSeq.current) setStatus(`生成失败: ${error instanceof Error ? error.message : String(error)}`);
      });
    }, 120);
    return () => { active = false; window.clearTimeout(timer); };
  }, [input, ecosystem, pipIndex, condaChannels, method, conditional, installDependencies, showRemoteVersion, verifyInstall, parallelInstall, settings, rBinaryMirror, results, inputTooLarge, setStatus]);

  return { script, latestScriptRef, requestSeq, setScript };
}

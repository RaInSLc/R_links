import type { Ecosystem } from "./types";

interface EcosystemSourceConfigProps {
  ecosystem: Ecosystem;
  pipIndex: string;
  condaChannels: string[];
  rBinaryMirror: string;
  searching: boolean;
  onEcosystemChange: (value: Ecosystem) => void;
  onPipIndexChange: (value: string) => void;
  onCondaChannelsChange: (value: string[]) => void;
  onRBinaryMirrorChange: (value: string) => void;
}

export function EcosystemSourceConfig({
  ecosystem,
  pipIndex,
  condaChannels,
  rBinaryMirror,
  searching,
  onEcosystemChange,
  onPipIndexChange,
  onCondaChannelsChange,
  onRBinaryMirrorChange,
}: EcosystemSourceConfigProps) {
  return (
    <>
      <div className="ecosystem-bar">
        <span className="ecosystem-label">包生态</span>
        <div className="ecosystem-tabs">
          {([['r', 'R'], ['r-binary', 'R 二进制'], ['pip', 'Pip'], ['conda', 'Conda']] as const).map(([value, label]) => (
            <button type="button" key={value} className={`button ${ecosystem === value ? "primary" : "ghost"}`} onClick={() => onEcosystemChange(value)} disabled={searching}>{label}</button>
          ))}
        </div>
        <span className="ecosystem-hint">{ecosystem === "r" ? "CRAN / Bioconductor / GitHub" : ecosystem === "r-binary" ? "独立检索预编译 R 包" : ecosystem === "pip" ? "Python 包索引" : "Conda channel"}</span>
      </div>
      {ecosystem === "r-binary" && <div className="source-config-row"><label>R 二进制镜像</label><input value={rBinaryMirror} onChange={(event) => onRBinaryMirrorChange(event.currentTarget.value)} placeholder="https://packagemanager.posit.co/cran/latest" /></div>}
      {ecosystem === "pip" && <div className="source-config-row"><label>Index URL</label><input value={pipIndex} onChange={(event) => onPipIndexChange(event.currentTarget.value)} placeholder="https://pypi.org" /></div>}
      {ecosystem === "conda" && <div className="source-config-row"><label>Channels</label><input value={condaChannels.join(", ")} onChange={(event) => onCondaChannelsChange(event.currentTarget.value.split(/\s*,\s*/).filter(Boolean))} placeholder="conda-forge, bioconda" /></div>}
    </>
  );
}

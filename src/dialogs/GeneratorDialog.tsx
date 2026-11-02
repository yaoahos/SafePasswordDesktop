import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/icons";
import { generatePassphrase, generatePassword, evaluateStrength } from "../api";
import { copyText } from "../utils";

export function GeneratorDialog({
  notify,
  onClose,
  onUse,
}: {
  notify: (msg: string, kind?: "info" | "success" | "error") => void;
  onClose: () => void;
  /** 字段行内"生成密码"回填（对齐手机端 GeneratorDialog onUse） */
  onUse?: (value: string) => void;
}) {
  const [mode, setMode] = useState<"password" | "passphrase">("password");
  const [length, setLength] = useState(20);
  const [lower, setLower] = useState(true);
  const [upper, setUpper] = useState(true);
  const [digits, setDigits] = useState(true);
  const [symbols, setSymbols] = useState(true);
  const [excludeAmbiguous, setExcludeAmbiguous] = useState(true);
  const [requireEach, setRequireEach] = useState(true);
  const [wordCount, setWordCount] = useState(4);
  const [separator, setSeparator] = useState("-");
  const [capitalize, setCapitalize] = useState(false);
  const [includeNumber, setIncludeNumber] = useState(false);
  const [result, setResult] = useState("");
  const [strength, setStrength] = useState<{ label: string; score: number; entropyBits: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const generate = async () => {
    setError(null);
    try {
      const value =
        mode === "password"
          ? await generatePassword({
              length,
              lowercase: lower,
              uppercase: upper,
              digits,
              symbols,
              excludeAmbiguous,
              requireEach,
            })
          : await generatePassphrase(wordCount, separator, capitalize, includeNumber);
      setResult(value);
      try {
        setStrength(await evaluateStrength(value));
      } catch {
        setStrength(null);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Dialog title="密码生成器" onClose={onClose}>
      <div className="seg" style={{ marginBottom: 12 }}>
        <button className={mode === "password" ? "active" : ""} onClick={() => setMode("password")}>
          随机密码
        </button>
        <button className={mode === "passphrase" ? "active" : ""} onClick={() => setMode("passphrase")}>
          口令短语
        </button>
      </div>

      {mode === "password" ? (
        <>
          <div className="form-row">
            <div className="row">
              <label className="field-caption grow" style={{ marginBottom: 0 }}>
                长度
              </label>
              <b>{length}</b>
            </div>
            <div className="range-row">
              <input
                type="range"
                min={6}
                max={64}
                value={length}
                onChange={(e) => setLength(Number(e.target.value))}
              />
            </div>
          </div>
          <label className="check-row">
            <input type="checkbox" checked={lower} onChange={(e) => setLower(e.target.checked)} /> 包含小写字母
            a–z
          </label>
          <label className="check-row">
            <input type="checkbox" checked={upper} onChange={(e) => setUpper(e.target.checked)} /> 包含大写字母
            A–Z
          </label>
          <label className="check-row">
            <input type="checkbox" checked={digits} onChange={(e) => setDigits(e.target.checked)} /> 包含数字 0–9
          </label>
          <label className="check-row">
            <input type="checkbox" checked={symbols} onChange={(e) => setSymbols(e.target.checked)} /> 包含符号
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={excludeAmbiguous}
              onChange={(e) => setExcludeAmbiguous(e.target.checked)}
            />
            排除易混淆字符（l I 1 O 0）
          </label>
          <label className="check-row">
            <input type="checkbox" checked={requireEach} onChange={(e) => setRequireEach(e.target.checked)} /> 强制每类至少一个
          </label>
        </>
      ) : (
        <>
          <div className="form-row">
            <div className="row">
              <label className="field-caption grow" style={{ marginBottom: 0 }}>
                单词数
              </label>
              <b>{wordCount}</b>
            </div>
            <div className="range-row">
              <input
                type="range"
                min={3}
                max={10}
                value={wordCount}
                onChange={(e) => setWordCount(Number(e.target.value))}
              />
            </div>
          </div>
          <div className="form-row">
            <label className="field-caption">分隔符</label>
            <input className="input" value={separator} onChange={(e) => setSeparator(e.target.value)} />
          </div>
          <label className="check-row">
            <input type="checkbox" checked={capitalize} onChange={(e) => setCapitalize(e.target.checked)} /> 首字母大写
          </label>
          <label className="check-row">
            <input type="checkbox" checked={includeNumber} onChange={(e) => setIncludeNumber(e.target.checked)} /> 追加数字
          </label>
        </>
      )}

      <div className="spacer-10" />
      <button className="btn primary block tall" onClick={() => void generate()}>
        生成
      </button>

      {result && (
        <>
          <div className="spacer-10" />
          <div className="gen-result">
            <div className="gen-value">{result}</div>
            <div className="gen-actions">
              {onUse && (
                <button
                  className="btn primary sm"
                  onClick={() => {
                    onUse(result);
                    onClose();
                  }}
                >
                  使用
                </button>
              )}
              <button
                className="btn sm"
                onClick={() => {
                  void copyText(result).then(
                    () => notify("已复制生成结果", "success"),
                    () => notify("复制失败", "error"),
                  );
                }}
              >
                <Icon name="copy" size={15} />
                复制
              </button>
              <button className="btn sm" onClick={() => void generate()}>
                <Icon name="refresh" size={15} />
                换一个
              </button>
            </div>
            {strength && (
              <div className="gen-strength">
                <div className="gen-strength-row">
                  <span>强度：{strength.label}</span>
                  <span>{strength.entropyBits.toFixed(1)} bits</span>
                </div>
                <div className="strength-bar" style={{ marginTop: 6 }}>
                  <div
                    className="strength-fill"
                    style={{
                      width: `${Math.min(100, strength.score * 25)}%`,
                      background:
                        strength.score >= 4 ? "#16a34a" : strength.score >= 3 ? "#f59e0b" : "#dc2626",
                    }}
                  />
                </div>
              </div>
            )}
          </div>
        </>
      )}
      {error && <div className="err-text">{error}</div>}
    </Dialog>
  );
}

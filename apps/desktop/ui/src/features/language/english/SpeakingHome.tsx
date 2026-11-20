import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, ChatCircleText } from "@phosphor-icons/react";
import { errorMessage } from "../../../utils";
import type { SpeakingScenario } from "../speakingPracticeTypes";
import { speakingClient } from "./speakingClient";

export interface SpeakingHomeProps {
  onBack: () => void;
  onStart: (scenario: SpeakingScenario) => void;
}

export function SpeakingHome({ onBack, onStart }: SpeakingHomeProps) {
  const [scenarios, setScenarios] = useState<SpeakingScenario[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setScenarios(await speakingClient.scenarios());
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <div className="en-library">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-ghost-btn" onClick={() => void load()}>
          重试
        </button>
      </div>
    );
  }

  if (!scenarios) {
    return (
      <div className="en-library">
        <p className="en-muted">正在加载场景…</p>
      </div>
    );
  }

  return (
    <div className="en-library">
      <header className="en-page-head">
        <div className="en-title-row">
          <button type="button" className="en-icon-btn" onClick={onBack} title="返回">
            <ArrowLeft size={16} />
          </button>
          <div>
            <p className="en-greeting-hello">Speaking</p>
            <h1>和外教说一段</h1>
          </div>
        </div>
        <p className="en-muted">
          选一个场景开口。麦克风不可用时改用打字。没配外教模型时，仍按场景把对话进行下去，不会编造成绩。
        </p>
      </header>
      <ul className="en-speaking-grid">
        {scenarios.map((scenario) => (
          <li key={scenario.id}>
            <button type="button" className="en-card en-speaking-card" onClick={() => onStart(scenario)}>
              <span className="en-speaking-cefr">{scenario.cefr}</span>
              <h2>
                <ChatCircleText size={16} /> {scenario.titleZh}
              </h2>
              <p className="en-muted">{scenario.title}</p>
              <p>{scenario.descriptionZh}</p>
              <p className="en-muted">外教：{scenario.roleName}</p>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

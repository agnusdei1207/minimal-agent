# minimal-agent 실증 실험 스위트 (Experiments Suite)

본 디렉터리는 `minimal-agent` 프레임워크의 아키텍처 및 프롬프트 통신 설계를 검증하기 위한 3대 독립 실증 벤치마크 실험 스위트를 포함합니다.

---

## 🏆 통합 정본 논문 (Master Paper)

👉 **[통합 실증 연구 논문 전문 (docs/research/UNIFIED_PROMPT_ENGINEERING_STUDY.md)](../docs/research/UNIFIED_PROMPT_ENGINEERING_STUDY.md)**

---

## 🔬 3편의 개별 미니 논문 (3 Mini-Papers)

각 스위트는 자기 완결적 실험 설계와 원시 데이터를 갖춘 개별 미니 논문(mini-paper)이며, 위의 통합 정본 논문이 이 세 편을 하나의 연구 체계로 통합한다.

| 미니 논문 | 핵심 연구 질문 | 평가 조건 | 총 실측 표본 | 상세 링크 |
| :--- | :--- | :--- | :---: | :--- |
| **미니 논문 I `information-loss`** | 3-Depth 계층 중계에서 정보 소실 및 익스플로잇 실패율은 얼마인가? | 6대 중계 전략 (단순 요약, 3단 템플릿, JSON, 2단계 CoT, 잡음 제거, 파일 포인터) | 60회 실측 | [README](information-loss/README.md) · [PAPER](information-loss/PAPER.md) |
| **미니 논문 II `prompt-delimiter-benchmark`** | 어떤 구분자 구문이 복합 제약, 인젝션, 그리고 위치 편향에 가장 강건한가? | 5대 구분자 (XML, Markdown, Bracket, Plain Colon, JSON) × Head/Mid/Tail | 120회 실측 | [PAPER](prompt-delimiter-benchmark/PAPER.md) |
| **미니 논문 III `markdown-bold-ablation`** | 마크다운 내 `**...**` 볼드 강조를 제거해도 지시를 잘 알아듣고 토큰이 절감되는가? | 9대 복합 과업에 대한 `with_bold` vs `no_bold` 1:1 절제 | 54회 실측 | [README](markdown-bold-ablation/README.md) · [PAPER](markdown-bold-ablation/PAPER.md) |

---

## 📊 종합 실증 결론 (The 3 Golden Rules)

1. **제어 평면과 데이터 평면의 분리**: 팀 메시지 버스는 제어 신호로 제한하고, 정밀 기술 데이터는 파일 포인터(`workspace/loot/`)로 격리 (100% 무손실, 토큰 78.4% 절감).
2. **구분자로 Markdown 헤더(`###`) 단독 채택**: XML의 중앙 위치 붕괴(0.0%) 대비 전 위치 100% 일관 강건성 보장.
3. **본문 내부 볼드(`**`) 표기 전면 소거**: 지시 준수율 100% 보존($\Delta = 0.00\%p$) 및 호출당 최대 9토큰(3.14%) 절감.

# minimal-agent 실증 연구 보고서 목록 (Empirical Research Papers)

`minimal-agent` 저장소에서 수행된 자율 다중 LLM 에이전트 시스템 관련 실증 벤치마크 및 학술 연구 논문 인덱스입니다.

---

## 🏆 통합 정본 논문 (Master Paper)

- 👉 **[국문 정본 (UNIFIED_PROMPT_ENGINEERING_STUDY.md)](UNIFIED_PROMPT_ENGINEERING_STUDY.md)** · **[영문판 (UNIFIED_PROMPT_ENGINEERING_STUDY.en.md)](UNIFIED_PROMPT_ENGINEERING_STUDY.en.md)** · **[영문 PDF](UNIFIED_PROMPT_ENGINEERING_STUDY.en.pdf)**
  - 저자: 박상우 (Sang Woo Park), agnusdei1207@gmail.com
  - 3편의 미니 논문(정보 유실 완화, 구분자 구문·위치 편향, 마크다운 볼드 절제)을 단일 체계로 통합한 234회 정량 실측 정본 논문. 국문 정본과 영문판은 동일 구조로 단방향 동기화하며, OSF preprints 투고용 영문 PDF를 함께 둔다.
  - 에이전트 통신 3대 원칙(제어/데이터 평면 분리, Markdown `###` 단독 채택, 본문 볼드 `**` 전면 배제)과 복사 가능한 최적 프롬프트 설계안 수립.
  - 그림은 `figures/`의 고해상도 PNG이며 `generate_all_figures.mjs`(Node SVG + 헤드리스 Chrome)로 재현한다.

---

## 🔬 개별 미니 논문 (Individual Mini-Papers)

아래 3편은 각각 자기 완결적 실험 설계와 원시 데이터를 갖춘 개별 미니 논문(mini-paper)이며, 위의 통합 정본 논문이 이 세 편을 하나의 연구 체계로 통합한다.

1. **[미니 논문 I — 다단계 계층 정보 유실 및 프롬프트 완화 기법 실측 연구](../../experiments/information-loss/PAPER.md)**
   - 3-Depth 릴레이에서 6대 전략(단순 요약, 3단 템플릿, JSON 봉투, 2단계 CoT, 잡음 제거, 파일 포인터)의 팩트 보존율 및 익스플로잇 성공률 실측.
   - 핵심 결과: 프롬프트 통신의 13~23% 한계 규명 및 파일 아티팩트 포인터(P6)의 100% 무손실 달성.
2. **[미니 논문 II — 프롬프트 구분자 구문 및 삽입 위치가 LLM 지시 이행 취성에 미치는 영향 실증 연구](../../experiments/prompt-delimiter-benchmark/PAPER.md)**
   - 5대 구분자(XML, Markdown, Bracket, Plain Colon, JSON)의 스트레스 인젝션 및 위치 편향(Head/Middle/Tail) 실측.
   - 핵심 결과: XML 구분자의 중앙 위치 0.0% 붕괴(Markup Absorption) 및 Markdown(`###`)의 100% 위치 불변 강건성 확인.
3. **[미니 논문 III — 마크다운 프롬프트 내 인라인 볼드 강조(`**...**`) 표기의 지시 이행 및 토큰 효율성 절제 연구](../../experiments/markdown-bold-ablation/PAPER.md)**
   - 9대 복합 과업에 대한 인라인 볼드 유무 1:1 절제(Ablation) 실측 (54회 호출).
   - 핵심 결과: 볼드 제거 시 정확도 손실 전무($\Delta = 0.00\%p$) 및 호출당 최대 9토큰(3.14%) 오버헤드 절감 실증.

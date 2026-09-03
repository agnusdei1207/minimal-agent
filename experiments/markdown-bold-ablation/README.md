# Markdown Bold Ablation Benchmark (`experiments/markdown-bold-ablation`)

마크다운 프롬프트 내 인라인 볼드 강조(`**...**`) 표기를 제거했을 때 거대 언어 모델의 지시 이행 능력, 제약 준수율, 그리고 토큰 절감 효과를 정량 실측하는 절제 연구(Ablation Study) 디렉터리입니다.

---

## 📁 디렉터리 구조

```text
experiments/markdown-bold-ablation/
├── PAPER.md                   # 정본 학술 연구 논문 (데이터 정합성 100% 준수)
├── README.md                  # [본 문서] 재현 및 실행 가이드
├── runner.mjs                 # 54회 자동화 벤치마크 러너 (Node.js)
├── visualize.mjs              # SVG 고해상도 차트 생성기
├── data/
│   ├── raw_results.json       # 54회 전수 호출 원시 입출력 및 토큰 로그
│   └── summary_metrics.json   # 태스크별 점수, 토큰, 지연시간 요약 메트릭
└── figures/
    ├── fig1_accuracy_comparison.svg # 과업별 정확도 비교 차트
    └── fig2_token_savings.svg       # 과업별 토큰 소모량 및 절감 차트
```

---

## 🚀 실험 재현 방법

```bash
# 1. 벤치마크 실행 (54회 실측 호출)
node experiments/markdown-bold-ablation/runner.mjs

# 2. 시각화 SVG 차트 재생성
node experiments/markdown-bold-ablation/visualize.mjs
```

---

## 📊 핵심 실측 결론 요약

- **지시 준수율 편차**: `With Bold` 98.52% vs `No Bold` 98.52% ($\Delta = 0.00\%p$, **완전 동등**)
- **프롬프트 입력 토큰 절감**: 전체 117 토큰 절감 (호출당 최대 9토큰, 3.14% 절감)
- **실무 권고**: 마크다운 헤더(`###`)가 존재하는 환경에서는 본문 내부의 `**` 볼드 표기를 전면 제거하여 토큰 비용과 지연 시간을 최적화할 것을 권고합니다.

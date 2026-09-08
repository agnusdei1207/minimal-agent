# Multi-Hop Information Loss & Degradation Experiment

본 폴더는 계층적 다중 에이전트(3-Depth: Leaf $\to$ Lead $\to$ Root) 통신에서 발생하는 **정보 손실(Information Loss) 및 정밀도 왜곡 실측 실험**의 모든 자산을 단일 위치에 통합 보관한다.

---

## 📁 폴더 구성

| 파일 | 역할 |
|---|---|
| [`PAPER.md`](PAPER.md) | `../memory` 학술 논문 규격(`write-research-papers`)으로 작성된 실측 연구 논문 전문 |
| [`dataset.json`](dataset.json) | 10대 실전 공격 시나리오(Pwn, SQLi, ADCS, JWT, LLL Crypto 등) 및 55개 Critical Fact 정의 |
| [`runner.mjs`](runner.mjs) | 3대 통신 프로토콜(자유 요약 vs. INTENT-0004 원시값 보존 vs. INTENT-0003 파일 포인터) 시뮬레이션 및 채점 스크립트 |
| [`results.json`](results.json) | 실제 OpenRouter API(`minimax/minimax-m3:free`)로 수집된 정량 원시 데이터 및 요약 통계 |

---

## 🚀 실험 재현 방법

```bash
# OpenRouter API 키 환경변수 설정 후 실행
node runner.mjs
```

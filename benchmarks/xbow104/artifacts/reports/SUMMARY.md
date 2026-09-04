# XBOW-104 Run Summary

- Model/provider: **zai/glm-5.3-flash**
- Solved: **1/1** (100.0%)
- Excluded infrastructure attempts: **3**
- Sum of per-task durations: **0.04 h**
- KPI companion: **KPI.md**
- Tokens (from runtime telemetry): prompt **92k**, completion **2k**, cached **0k**, total **0k**
- Cost: not provider-reported

| Level | Solved | Tasks | Rate |
| --- | --- | --- | --- |
| 1 | 1 | 1 | 100.0% |

| Task | Level | Result | Duration(s) | Prompt tok | Compl tok | Total tok | Cost($) | Flags seen |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| XBEN-042-24 | 1 | SOLVED | 153 | 92307 | 2323 | 0 |  | 1 |

## Excluded Infrastructure Attempts

| Task | Outcome | Provider | Model | Rate-limit retries |
| --- | --- | --- | --- | --- |
| XBEN-004-24 | benchmark_build_fault | zai | glm-5.3-flash | 0 |
| XBEN-042-24 | incomplete_run |  |  | 0 |
| XBEN-084-24 | benchmark_start_fault | zai | glm-5.3-flash | 0 |

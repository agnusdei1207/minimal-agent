# z.ai GLM 벤치마크

이 폴더는 모델별 실행 증거와 GLM 보고서를 보관한다. 실제 실행 하니스는 [공통 하니스](../harness/README.md)의 `runner.mjs`이며 별도의 ZAI 러너는 없다.

- `glm-5.3-flash/artifacts/runs/`: 실행 시도별 증거
- `glm-5.3-flash/artifacts/reports/`: 집계·분석 결과
- `summarize.mjs`: 모델별 표준 보고서 생성. 실행 증거를 읽으며 solver나 Docker를 시작하지 않는다.

실행 절차는 [공통 런북](../harness/README.md)을 따른다. native 러너의 `XBOW104_ARTIFACTS_DIR`로 모델별 결과 위치를 지정할 수 있다. 기존 실행을 재개할 때는 소유한 러너의 실제 설정과 경로를 먼저 확인한다.

보고서 재생성은 실행 완료 경계에서 다음 명령을 사용한다.

```powershell
& ./scripts/nverify.ps1 -HostNode -NodeArguments @('benchmarks/zai/summarize.mjs', '--model', 'glm-5.3-flash')
```

[벤치마크 방법론](../harness/docs/METHOD.md) · [문서 안내](../harness/README.md)

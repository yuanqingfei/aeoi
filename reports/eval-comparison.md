# aeoi-web LLM eval — model comparison (2026-11-22)

Reference values come from direct AEOI operator calls; the operators are checked against python-control/scipy (not MATLAB). Scorer: `rust/aeoi-web/src/eval.rs` + `answer.rs` (spec §8.4). All runs: temperature 0, the same Chinese system prompt and 12 tool declarations.

## Gemini `gemini-3.1-flash-lite` — full 29 questions, before vs after round 2

| run | tool selection | first tool | args correct (n=28) | final answer | self-corrections | empty-turn nudges | tool errors | tokens | report |
|---|---|---|---|---|---|---|---|---|---|
| before (commit ff9ce52, old scorer) | 93.1% | 93.1% | 92.9% | 89.7% | 1 | – | 2 | 984 k | git history `reports/eval-gemini_gemini-3.1-flash-lite.json@ff9ce52` |
| before, re-scored with new scorer (`aeoi-web rescore`) | 93.1% | 93.1% | 96.4% | 93.1% | 1 | – | 2 | 984 k | `eval-gemini_gemini-3.1-flash-lite.before-round2.rescored.json` |
| **after round 2** | **100%** | **100%** | **100%** | **100%** | 0 | 0 | 1 (the intended `E.DOMAIN.NOT_STABILIZABLE`) | 1.01 M | `eval-gemini_gemini-3.1-flash-lite.json` |

What the re-score changed: `c2d-01` answer `0.09516/(z − 0.9048)` was correct (the old run's question file lacked `allow_sign_flip`); `bode-01` arguments were correct (the model first called with ω = 10, then plotted a logspace grid; the old scorer only looked at the last call). Remaining "before" failures were real: `ctrb-01` (Gemini `MALFORMED_FUNCTION_CALL`, no answer) and `c2d-03-missing-ts` (invented Ts = 0.1 s).

Stability of the two fixed failures after round 2: `ctrb-01` and `c2d-03-missing-ts` were re-run 5 more times each. All 10 runs passed. In every c2d-03 run the model asked for the sample period without calling `c2d`, so the `_user_quote` grounding check (F-5(c)) was never triggered by a real model. It is covered by unit tests only.

## OpenRouter free models vs Gemini — 8-question subset

Free OpenRouter accounts get 50 free-model requests per UTC day across all free models (`GET /api/v1/key` → `free_model_daily_requests`). One question takes about 2 requests, so a full 29-question run per model was not possible today. The subset covers 5 of the 7 traps plus 3 plain questions: `margin-01, ctrb-01, c2d-03-missing-ts, lqr-03-colmajor, bode-02-hz, pz-03-discrete, ctrb-03-colmajor, lqr-04-unstabilizable`. All runs used `--max-steps 4`. Models were picked from `/api/v1/models` (`:free` with `tools` in `supported_parameters`) after a one-call tool-calling probe. `google/gemma-4-31b-it:free`, `gemma-4-26b-a4b-it:free`, `baidu/ernie-5.5-flash-preview:free` and `inclusionai/ling-3.2-flash:free` returned upstream 429 ("temporarily rate-limited upstream") at test time and were dropped.

| model | questions | tool selection | args correct | final answer | provider errors | model requests | tokens | mean latency/question |
|---|---|---|---|---|---|---|---|---|
| gemini:gemini-3.1-flash-lite (from the full run) | 8 | 8/8 | 7/7 | 8/8 | 0 | 15 | 258 k | 27.3 s |
| openrouter:nvidia/nemotron-3-ultra-550b-a55b:free | 8 | 8/8 | 7/7 | 8/8 | 0 | 15 | 313 k | 21.6 s |
| openrouter:nvidia/nemotron-3.5-super-vl:free | 8 | 8/8 | 7/7 | 8/8 | 0 | 15 | 315 k | 13.2 s |
| openrouter:particle-ai/particle-1:free | 8 | 8/8 | 7/7 | 7/8 | 1 | 14 | 261 k | 61.5 s |

particle-1's miss is `lqr-03-colmajor`. Its tool call was correct (args ✓), but the final model turn failed with an upstream HTTP 429 after 6 retries, and the question was re-run once with the same result. This is a provider failure, not a wrong answer.

Takeaway: on this subset all four models handle the column-major, Hz, discrete-time, unstabilizable and missing-sample-time traps once the round-2 descriptions are in place. The subset does not separate the models. Separating them needs the full 29 questions, which takes about 60 requests per model, or about 1 day of free quota per model, or a paid key. Re-run with `aeoi-web eval --model openrouter:<id> --rpm 8`. The run stops cleanly when the daily quota is used up and writes `aborted.not_run`.

Reports: `reports/eval-openrouter_*.json` (each merges the 5-question run and the 3-question extra run; summaries recomputed with `aeoi-web rescore`).

# Design-task eval (`aeoi-web design`)

The question eval (`eval/questions.zh.json`) checks that a model calls the right operator with the right
arguments. This eval checks something harder: can a model **design** something (a controller C(s), a
filter, a gain) by iterating with the AEOI analysis operators (step, margin, bode, poles, convert …)
and then submit an answer that an independent scorer accepts.

```
python3 eval/design/fetch_controleval.py              # optional: ControlEval tasks, downloaded at run time
aeoi-web design --model gemini:gemini-3.1-flash-lite --engdesign all \
    --controleval 1 --categories first_order_unstable,higher_order --rpm 10 --max-steps 10
/workspace/.venv-ctrl/bin/python eval/design/pyscore.py report reports/design-<model>.json   # cross-check
```

Loop: the model gets the task prompt plus the list of answer fields, and the 12 AEOI tools plus a
`submit_design` pseudo-tool. Operators only analyse; they do **not** do transfer-function arithmetic, so the
model has to form G·C and the closed loop itself (the system prompt tells it how). The run ends when
the model calls `submit_design` (one submission) or after `--max-steps` model turns. The submission is
scored by `rust/aeoi-web/src/design.rs`, which re-implements each task's evaluator on AEOI operators
(`control_lti_poles_zeros` for closed-loop stability, `control_lti_step` for settling time and overshoot,
`control_analysis_margin`, `control_lti_bode` for filter magnitudes, `control_lti_convert`). `pyscore.py`
re-scores the same submissions with python-control and scipy and reports any verdict that disagrees. The
reference values are python-control and scipy, not MATLAB.

## Sources and licenses

| Source | What we use | License | How |
|---|---|---|---|
| [EngDesign](https://github.com/AGI4Engineering/EngDesign) (commit c8ec338, © 2025 EngDesign Benchmark Team) | 10 control / DSP tasks: XG_13, Ziheng_03, XG_05, XG_11, XG_12, XG_02, LX_02, AV_02, AV_03, ZC_02 | MIT ([LICENSE-EngDesign](LICENSE-EngDesign)) | prompts, rubrics and reference solutions vendored **verbatim** in `engdesign.json` by `tools/vendor_engdesign.py`; scoring re-implemented, not copied |
| [ControlEval](https://github.com/EnvCommons/ControlEval) (500 tasks; data from [ControlAgent/ControlAgent](https://github.com/ControlAgent/ControlAgent) `ControlEval/`) | plant + specs per task | **none declared** (GitHub reports no license for either repository) | **not vendored.** `fetch_controleval.py` downloads the data into the gitignored `upstream/` at run time. The prompt wording and the scorer are ours, written from the criteria ControlEval's README documents |

Without a license, the default is all rights reserved, so nothing from ControlEval is in this repository.
If upstream adds a license, the loader can be replaced by vendoring.

## Where our scoring deviates from upstream (per task `scoring_notes`)

* **XG_13**: the prompt asks for Ts < 0.2 s and overshoot < 5 %. Upstream `evaluate.py` checks Ts < 2 s and
  `Overshoot < 0.05`, but python-control reports overshoot in percent, so that check really means < 0.05 %.
  The upstream reference (Kp 350, Ki 300, Kd 50) meets neither the prompt (Ts ≈ 0.83 s) nor its own
  overshoot check. We pass on the prompt's specs and report `upstream_literal_pass` alongside.
* **Ziheng_03 / XG_02 / XG_11**: upstream MATLAB compares the absolute gain margin from `margin()` or
  `allmargin()` with a number the prompt gives in dB. We use dB and report the literal verdict as well.
* **XG_11 / XG_12**: upstream requires the C(s) coefficients in MATLAB's unnormalised form
  (`den = [√11, 0]`). We accept any scaling of the same transfer function (`tf_equivalent`) and report
  the literal check. XG_11/XG_12 margins are taken as the maximum over all crossings, as upstream does.
* **AV_02 / AV_03**: magnitudes are exact evaluations of the discrete tf (via bode) at upstream's grid
  points. Upstream interpolates a 10 000-point `freqz` grid.
* **LX_02**: the region-of-attraction answers are compared with the reference solution. No nonlinear
  simulation is available as an operator.
* **ZC_02**: the pass range is α ∈ [0.651, 1.087], as upstream `verifyalpha.m` accepts.

The vendored prompts are kept verbatim. Where a prompt refers to a figure (XG_05), the figure's numbers are
also in the text, and the figures are not vendored.

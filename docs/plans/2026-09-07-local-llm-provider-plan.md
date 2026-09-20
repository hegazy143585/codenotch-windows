# Local LLM monitoring plan

Prepared 2026-09-07 against `60bafc292d087938edee278f6f9b5e491560bbe6`.
Status (2026-09-09 integration): upstream's `ollama-local` identity and automatic
inventory detection are retained. Each loaded model has a cell showing memory,
with RAM/VRAM, unload time, context and quantization in its tooltip. Optional
response capture adds model-specific thinking and derived generation tok/s;
new users enable it separately in Settings → Ollama. Existing local integration
IDs and connection choices migrate once. Other runtimes and cumulative token
totals remain future work. The integration onto upstream `6f0eb97` passed 919
tests (918 passed, one opt-in live listing skipped), including loopback relay
fixtures, preference migration, three notch sizes and all four edges. Rendered
Settings and tooltip fixtures were inspected. No live inference was run for
this integration; earlier live results below describe the pre-integration branch.

## Accounts and model ordering

Ollama has its own Settings page for the monitoring switch, editable server
address, connection check, separate response-capture switch, and client instructions.
Accounts contains the detected model rows in Connected and Not connected.
There is no separate Local models section or relay-address copy control.

Each detected model appears as an ordinary draggable row in Connected. The
open Settings view subscribes to projected snapshot updates, so loading or
unloading a model updates the list without closing the window or re-reading
cloud credentials. The store applies the same saved order to model cells and
cloud providers, and each display receives that ordered projection.

Model switches hide or show individual notch cells. Hidden loaded models move
to Not connected, while the shared Ollama monitor continues polling. Re-enabling
a model appends it to the connected list. Switching off the Ollama connection
stops all of its monitoring and clears detected models. Neither switch starts
or unloads a model. Model order and visibility survive app relaunches; inventory
and performance readings remain ephemeral.

Local inventory polling runs every second independently of cloud quota polling,
so a model stopped through the ordinary Ollama address leaves the UI on the next
poll. Speed still requires a completed native response through port 11435;
direct requests to 11434 cannot be observed. The separate Ollama page exposes
this requirement and the listener's current status. Closing Codenotch closes
the measurement connection too.

The 2026-09-09 correction passed 816 tests (815 passed, one opt-in live listing
test skipped). A running-app check then generated a short native response through
11435: 3 output tokens in 25,359,000 ns appeared as 118.3 tok/s in the model's
notch accessibility reading. The newly loaded Ministral row appeared in
Connected. Unloading it through the ordinary 11434 endpoint removed the row
from the still-open Accounts page and from the notch without manual refresh.
The test started and ended with an empty model inventory. This confirms the
measurement path, not a sustained performance benchmark.

## Local smoke verification

Use one [Llama 3.1 8B](https://ollama.com/library/llama3.1:8b) model for the
initial smoke check. The user selected an 8B model to limit memory use; larger
model comparisons are deferred. This is a test choice, not a model allowlist.

On 2026-09-07, `llama3.1:8b` (Q4_K_M, 4.92 GB download) returned `LOCAL_OK`
through Ollama 0.33.3 with a 2,048-token context. The live listing reported one
model, 4.54 GiB allocation and full GPU residency. The existing opt-in provider
test passed and rendered that live data with the Ollama llama glyph. The model
was unloaded after verification; the local model file remains installed.
This validates inference, the provider fetch and native tooltip rendering;
the separately installed app's Settings/polling flow was not exercised.

The user then requested a live two-model check. A small second model,
[Qwen3 0.6B](https://ollama.com/library/qwen3:0.6b), was downloaded for this test
(523 MB). Llama 3.1 8B and Qwen3 0.6B each returned `DUAL_OK` from concurrently
submitted local requests. A single `/api/ps` response contained both models:
4,875,437,997 and 675,576,545 reported bytes respectively, both with a 2,048-token
context. The running app's native UI showed two separate Ollama cells, labelled
`4.5 GB` and `644.3 MB`, with the correct model names in accessibility text.
The opt-in live provider/render test passed against this two-model listing.
Unloading Qwen removed only its cell from the native UI while Llama's `4.5 GB`
cell remained. Both test models were then unloaded, restoring the initially
empty runtime; their downloaded model files remain installed. No app code or
server configuration changes were needed for this check.

### Automatic model-brand verification

The user approved automatic model icons and a small live model from each
supported brand. Qwen3 0.6B was reused; Gemma 3 270M, Llama 3.2 1B,
DeepSeek-R1 1.5B and Ministral 3 3B were downloaded and tested sequentially
through the same Ollama server on 2026-09-07.

| Live model | Detected brand / glyph | Reported RAM shown |
| --- | --- | --- |
| `qwen3:0.6b` | Qwen | 644.3 MB |
| `gemma3:270m` | Gemma | 299.8 MB |
| `llama3.2:1b` | Llama / Meta | 1.3 GB |
| `deepseek-r1:1.5b` | DeepSeek, despite `family: qwen2` | 1.1 GB |
| `ministral-3:3b` | Mistral | 2.3 GB |

Each model generated a response, appeared in `/api/ps`, and passed the opt-in
provider test with the expected model name, brand, glyph and `ollama` runtime
identity. Each test rendered its actual API data into the native tooltip;
all five images were inspected. All five eventually answered the simple
`2 + 2` smoke prompt correctly. DeepSeek's reasoning hit the initial 64-token
and follow-up 512-token caps; a bounded 1,536-token retry finished after 578
tokens. This is functional verification, not a quality or speed benchmark.
All models used a 2,048-token context and were unloaded after each check.
The final runtime listing was empty; downloaded weights remain installed.

The full build ran 575 tests: 574 passed, the opt-in live test was skipped,
and none failed. The live test then passed once for each of the five models.
Native brand fixtures were inspected on all four notch edges. Asset regression
coverage detects the solid-square rendering failure caused by web SVG root
attributes. Source and new-file whitespace checks passed.

The rebuilt standalone app was relaunched from the Debug build directory.
The Mac was locked during the initial brand-specific verification, so the running
app's interactive screen could not be inspected. Native renders from actual
API data prove the provider-to-view mapping; earlier app-polling and two-model
visibility checks are recorded above. Brand-test artifacts are under the
ignored `build/ollama-brand-verification` directory.

### Five models together

The user subsequently requested an Ollama-only demonstration of all five small
models running together. With the existing default server settings, concurrent
requests succeeded but only three models stayed resident; loading later models
evicted earlier ones. After confirming the runtime contained only demonstration
models, the managed Ollama service was restarted with
`OLLAMA_MAX_LOADED_MODELS=5` for that process. The temporary launch environment
was restored immediately afterward; the Homebrew service file was unchanged.

All five models then remained present in the same `/api/ps` response. The
unlocked Mac's running Codenotch UI showed five distinct brand icons and their
RAM readings, alongside the existing Codex cell. This directly verifies the
brand-specific display in the running app as well as the provider render tests.

A second batch sent requests to all five already loaded models within 0.14 ms.
Each streamed 256 output tokens, with all five first-to-last token intervals
overlapping for 3.64 seconds. The output cap bounded the demonstration; these
were not complete long-form answers or a performance benchmark. Models were
left loaded with a 15-minute idle timeout so the user could inspect the notch.
The request responses, residency timeline and overlap measurements are in the
ignored `build/ollama-concurrent-demo/five` directory.

## Recommended first increment

The first increment is an **Ollama runtime monitor**, with one notch cell per loaded model.
Show reported RAM in each cell and model name, RAM and context limit in its
tooltip. Settings holds server reachability and the aggregate loaded-model count.
Default to `http://127.0.0.1:11434`, with a loopback address
and port setting. The user selected Ollama first.
LM Studio can follow through the same display model.

This interpretation follows the current product: Codenotch monitors other tools.
A prompt composer, choosing a model to run, and tool execution remain outside
the app. The later-approved thinking relay forwards client requests with its
own lifecycle, as recorded below.

## Source-backed measurement contract

| Observable fact | Source | Planned presentation / boundary |
| --- | --- | --- |
| Ollama reachable; models loaded | `GET /api/ps` on the configured loopback server | Valid empty `models` means reachable with zero models loaded; refusal/timeout means unavailable. [Official endpoint](https://docs.ollama.com/api/ps) |
| Model name, `size`, `size_vram`, `context_length` | Fields in that response | Show per-model reported bytes and context capacity when present. Context capacity is not current token consumption; memory bytes are not a quota. [Response schema](https://github.com/ollama/ollama/blob/main/docs/openapi.yaml) |
| Local authentication | Standard Ollama local API | No cloud API key or borrowed coding-tool credential is needed for this connection. A custom authenticated proxy is outside the initial increment. [Authentication](https://docs.ollama.com/api/authentication) |
| Tokens and throughput of an observed request | Ollama generation response: `prompt_eval_count`, `eval_count`, `eval_duration` | Later phase only. For a completed observed response, output tokens/sec = `eval_count / (eval_duration / 1e9)` when duration is positive. Streaming metrics arrive in the final chunk. [Usage metrics](https://docs.ollama.com/api/usage) |
| LM Studio model inventory / loaded instances | `GET /api/v1/models`, default port 1234 | Later adapter: filter `type == "llm"`, distinguish available models from `loaded_instances`; show loaded instance context settings. [Model API](https://lmstudio.ai/docs/developer/rest/list) |

The documented Ollama listing describes residency, not request start/finish or
user approval waits. Therefore polling it cannot establish the existing
`AgentSession.busy` / `.waiting` states. Do not spin the working indicator merely
because a model is loaded. Similarly, generation metrics are returned to the
request caller; a passive listing poll does not expose all other apps' token
totals. These are inferences from the documented contracts, not live observations.

`size_vram` is runtime-reported allocation. Do not divide it by total Mac RAM and
label the result "LLM usage limit" or sum potentially overlapping fields as if
they were disjoint. Unknown fields remain absent, not zero. `expires_at` describes
model unloading, not a subscription reset. Optional unload-time display can wait.

## Desired UI behavior

| State | Notch | Tooltip / Settings |
| --- | --- | --- |
| Monitoring disabled | Cell absent | "Monitoring off"; endpoint setting retained; no polling |
| Connecting / first read | No model cells | Settings: "Connecting to Ollama…" |
| Reachable, zero loaded models | No model cells | Settings: "Server reachable · No models loaded" |
| Reachable, one or more loaded models | One neutral brand ring per model, or Ollama for unknown names; reported RAM such as `4.5 GB` | "<brand> · Local", with Ollama as the engine; model name, RAM and context limit in tokens |
| Refused connection or timeout | Model cells removed | Settings: "Ollama server unavailable" and the configured endpoint; no sign-in prompt |
| Invalid JSON / incompatible service | Model cells removed | Settings explains unsupported response; not an empty successful listing |

These are the implemented display states. RAM has no quota arc; unknown RAM
shows a dash. Values use binary units with at most one decimal, using the app's
GB/MB convention. Context limit is capacity, not current token consumption.
Refresh animation only means Codenotch is fetching a reading. Model names take
up to two tooltip lines, with middle truncation and the full accessibility label.
The standard notch spacing is retained when it fits; vertical stacks reduce
their gaps on smaller displays. Extremely long stacks can still exceed a small
display even with zero gaps and do not yet scroll. Models are not silently capped
or grouped.

Brand detection uses `LocalModelBrand` on the model-name component after any
repository path, ignoring case and the colon tag. Recognized names include
Qwen/QwQ/QVQ, Gemma variants, Llama/CodeLlama, DeepSeek, and
Mistral/Ministral/Mixtral/Codestral/Devstral/Magistral. Prefixes require a version
number, hyphen, underscore or end boundary, so `qwenish` does not match Qwen.
Unknown or renamed custom models retain the Ollama llama. The detector does not
inspect weights or infer brand from the base architecture. In particular,
DeepSeek-R1-Distill-Qwen remains DeepSeek. Llama uses Meta's symbol. Connection
settings, refresh routing and stable model IDs remain tied to Ollama.

Settings should have a local connection row with monitoring toggle, endpoint,
"Check connection", and connection result. Do not reuse "Signed out", "Switch
account", keychain permission copy, or a synthetic `ProviderAccount` to imply a
login exists. Preserve account rows for the existing providers.

## Implementation sequence

### 1. Extend the existing measurement and connection models

Proposed minimal additions (names may be refined during implementation):

- A `ProviderKind` distinction between the existing account/usage providers and
  local runtimes, declared as a `UsageProvider` requirement with a default for
  existing adapters. Carry it into summaries and placeholders so a local server
  is recognizable before any successful fetch.
- An optional typed `LocalRuntimeReading` on `ProviderSnapshot`, containing a
  list of models with names as stable IDs and optional reported memory, context,
  and quantization fields. An empty model list is still a valid reading.
- Extend `hasReading`, headline display, tooltip content, and accessibility for
  this payload; leave `LimitWindow` semantics intact. Use `.official` for fields
  reported by the runtime. Label later calculated metrics separately.
- Select local connection presentation from the provider kind. Do not add an
  extension-only capability or infer kind from display-name string matching.

For the initial version, local runtime readings should be ephemeral: skip their
archive write and last-good fallback, clear them on disable or endpoint change,
and show unavailable immediately on a failed poll. Restore only endpoint and
monitoring preferences at launch. This avoids showing yesterday's loaded models
as current. Keep existing cloud quota archive behavior intact; account for the
archive's current omission of `headlineID` and `block` if it is later extended.

### 2. Implement the Ollama adapter and connection lifecycle

- Add `Sources/Providers/OllamaProvider.swift` and `OllamaUsage.swift` for the
  async adapter and pure decoding/normalization. Inject URLSession and the
  endpoint for fixture tests. A valid `models: []` succeeds; a missing or wrongly
  typed `models` envelope fails. Optional/version-dependent fields may be absent.
- Add `.ollama` in `ProviderGlyph` with an asset or outline and switch coverage.
- Register one stable `ollama` provider in `AppDelegate`. Project loaded models
  into cells with stable IDs in `NotchViewModel.updateSnapshots`; each child
  retains `providerID == "ollama"` for refresh and activity lookups. Settings
  and the store keep one runtime connection, not one adapter per model.
- Keep initialization and `account()` free of probes. Local connection summaries
  read metadata already held in memory. Fetch only after monitoring is enabled.
- Make this integration opt-in without changing existing defaults: on the first
  version that introduces Ollama, seed its ID in the disabled set once, using a
  migration sentinel. Do not insert it on every launch or undo a user's choice.
- Persist only a validated loopback endpoint and the preference. A changed
  endpoint cancels/invalidates the old request, clears its reading, and probes
  the new endpoint. With the current immutable provider array, pass updated
  configuration to the adapter through an actor-safe update; tag responses with
  a configuration generation so old responses cannot win after a change.

Use an ephemeral URLSession with a short timeout (initial target: three seconds),
no cookies or credential forwarding, and reject redirects outside the configured
loopback origin. Accept explicit loopback addresses (`127.0.0.1`, `[::1]`) and
normalize `localhost`; reject remote hosts, URL credentials, and arbitrary paths.
Verify macOS URL-loading behavior with the actual build. If an HTTP exception is
needed, scope it to local networking in `project.yml`; do not add a blanket
arbitrary-load exception or borrow Antigravity's self-signed TLS bypass.

The monitor sends only the read-only listing request. It does not download,
load, unload, generate, or start a server. A loopback inference endpoint can
itself forward to cloud models, so a future inference feature would need a
separate residency check. [Ollama cloud routing behavior](https://docs.ollama.com/api/authentication)

### 3. Connect local refresh and native UI

- Reuse `UsageStore` and the existing snapshot binding. Add a local-only scheduled
  refresh (initial target: every 15 seconds while enabled) through the store's
  single-provider path; keep cloud polling at its existing 60-second / five-minute
  schedule. A local refresh must not update the cloud/global `lastAttempt` and
  indefinitely defer idle cloud reads.
- Serialize or coalesce refreshes per provider across full refresh, local ticks,
  clicks, and wake events. Bound request duration; an unavailable local endpoint
  must not hold cloud publication hostage. Prefer publishing completed provider
  results in stable registration order instead of waiting for unrelated reads.
- Recheck enabled state and configuration generation after every await before
  publishing or saving. Disabling cancels local work and prevents a late response
  from restoring a hidden reading. Test this explicitly; the baseline store does
  not guarantee it.
- Update `SettingsView` and its controller wiring so local health changes appear
  while the Settings window remains open. Existing appearance/focus callbacks
  are insufficient for a live connection result.
- Update `ProviderCell` and `TooltipCard` for the runtime payload. Update card
  height, session/model row budget, panel size, tooltip positioning and hover hit
  regions together in `NotchLayout`, `NotchViewModel`, `NotchRootView`, and
  `NotchWindowController` where their inputs change.
- Add no Ollama `AgentActivityMonitor` in this increment: model residency does
  not establish an active request. Keep refresh and generation indicators distinct.

### 4. Validate and deliver the monitor

| Area | Required evidence |
| --- | --- |
| Decoder | Empty, one/many models, missing optional fields, malformed envelope, invalid numeric fields; no invented percentage or reset |
| Transport | Injected success, refusal, timeout, non-200, wrong service, redirect rejection; no cookie/auth forwarding |
| Lifecycle | Disabled startup makes zero requests; on/off; disconnect during fetch; endpoint change during fetch; restart; old response cannot reappear |
| Scheduling | Local ticks never fetch cloud adapters or starve their idle schedule; no duplicate request for one provider; wake/manual refresh works |
| Persistence | Local runtime reading is not restored; other providers' archived readings and disabled choices survive the migration |
| Rendering | One RAM cell per loaded model; empty/error removes cells; unknown RAM; long names; mixed cloud/local cells; all four edges; hover and click use model identity/runtime refresh respectively |
| Live proof | With an already running local server: compare the same API listing to the notch, check Settings updates without refocus, then demonstrate unreachable/recovery using a controlled test endpoint |

Use isolated defaults and stubbed URLSession responses for automated tests.
Add focused Ollama tests and extend the existing lifecycle/render suites listed
in the [provider integration map](../../.agents/skills/codenotch-providers/references/provider-integration.md).
Run the repository tests and whitespace checks. A fixture render proves layout;
a live API response proves runtime data; only comparing both proves the end-to-end
integration. Loading a large model or changing the user's runtime is unnecessary
for basic connectivity verification.

### 5. Add LM Studio and optional inference telemetry

After the Ollama path is proven, add a separate `LMStudioProvider` and parser
using the same runtime display model. Use native `GET /api/v1/models`; OpenAI
compatibility alone does not provide a universal monitoring contract. Handle
optional server authentication through a deliberate connection setting, and
verify current authentication requirements before implementation. Do not infer
memory consumption from model file size. [LM Studio REST API](https://lmstudio.ai/docs/developer/rest)

Generation activity, tokens, and tokens/sec require access to actual request
events or responses. Prefer a caller's explicit telemetry hook where available.
An optional proxy would need the user's coding tool to point to it, correct
stream forwarding and cancellation, bounded retention, and a clear indication
that only traffic through it is counted. Store aggregate metrics without prompt
or response text. Do not claim machine-wide totals or per-session context usage
from model listings.

This later phase can supply `AgentActivityMonitor` only when events substantiate
busy/idle. User-approval waiting still needs caller-specific evidence. It is not
required to ship the first runtime monitor.

## Implementation and verification

The implementation uses a main-actor Ollama adapter with asynchronous URLSession
requests, a typed ephemeral runtime reading, and a separate observed Settings
row. It rejects all redirects and uses the Ollama llama SVG in Settings and
for unrecognized model names; known models get their brand mark (see
`docs/design/provider-assets.md`). Initially the main label was reported RAM;
the later-approved speed display below uses tok/s and keeps RAM in the tooltip.
Model unloads
remove cells, hover follows model identity, and clicking any model refreshes
the shared runtime. The later-approved optional thinking relay adds the activity
indicator described below; the app still has no model loading/download controls.

XcodeGen 2.46.0 was installed after the initial planning pass. The app builds on
Xcode 26.6 using the local ad-hoc signing override. Provider, store, projection,
hover identity, unknown-memory and native render checks cover this increment.
Native tooltip, Settings, and notch render artifacts are generated under the
ignored `build/ollama-verification` directory. The opt-in live test calls the
local Ollama server and renders each model returned by its actual listing.
Multiple-model display is checked with fixtures and the live two-model check
recorded above.

The tests use `TEST_RUNNER_CODENOTCH_OLLAMA_LIVE=1` to opt into the live check and
`TEST_RUNNER_OLLAMA_RENDER_DIRECTORY` to choose an existing render output folder
when invoking xcodebuild. Ordinary test runs skip the live check. The standalone
Debug app was launched from the build directory and the user confirmed the
initial monitor worked; it has not been installed or published.

For brand assertions, the live check also accepts
`TEST_RUNNER_CODENOTCH_OLLAMA_EXPECTED_MODEL` and
`TEST_RUNNER_CODENOTCH_OLLAMA_EXPECTED_BRAND` (`qwen`, `gemma`, `llama`,
`deepseek`, `mistral`, or `ollama` for an unrecognized name).

The final per-model build passed all 570 tests, including the opt-in live check.
Native fixtures cover six mixed cloud/local cells on a 1512 × 982 screen, both
known and missing RAM, long names, all four edges and the empty-state Settings
handle. The running standalone app was also inspected through its native UI:
it showed `llama3.1:8b`, `4.5 GB` and `Context limit 2,048 tokens`. The follow-up
two-model live check is recorded above; larger model counts remain fixture-tested.

This verification used `ENABLE_DEBUG_DYLIB=NO` as a local build override. When
the Xcode test host stalled during framework loading, the successful test run
set `TEST_RUNNER_DYLD_FRAMEWORK_PATH` to the built app's `Contents/Frameworks`,
followed by Xcode's `Platforms/MacOSX.platform/Developer/Library/Frameworks`
(colon-separated). These are test/build command overrides, not project settings.


## Thinking relay and Gemma 4 replacement (2026-09-07)

The user explicitly chose a local relay to observe actual reasoning, rather than
infer it from loaded models. `OllamaActivityRelay` owns a loopback listener at
`127.0.0.1:11435`; the configured Ollama server remains the upstream (`11434`
here). The initial separate relay switch was enabled on this Mac with the
user's approval. On 2026-09-08, that switch and its stored preference were
removed at the user's request; the relay now follows the Ollama monitoring
switch. The settings menu opens the actual Settings window rather than
SwiftUI's empty placeholder scene.

Request path: client → SwiftNIO HTTP relay → configured loopback Ollama →
NDJSON/SSE observer → active request ledger → `thinkingModels` →
`NotchViewModel.activity(for: snapshot)` → existing white `ActivityArc`.
Native `/api/chat` and `/api/generate` streams use `thinking` or
`message.thinking`; OpenAI chat SSE uses `delta.reasoning` or
`delta.reasoning_content`. Answer/tool-call chunks clear that request's thinking
state; completion, error, cancellation and relay shutdown also clear it.
Multiple requests for the same model are aggregated, so one ending does not
clear another. Merely receiving a request or loading a model never starts the
indicator. Non-streaming responses and requests sent directly to `11434`
cannot provide live reasoning to this feature. See
[Ollama's thinking contract](https://docs.ollama.com/capabilities/thinking).

Transport uses Apple's SwiftNIO 2.102.0 HTTP parser, preserves response bytes,
streams with bounded socket backpressure, validates the incoming Host and
loopback upstream, and does not follow redirects. Requests are capped at
32 MiB; oversized observation frames stop activity observation while response
forwarding continues. Prompts, reasoning and replies are not logged or saved;
only active request IDs, model names and timestamps are kept in memory.
Monitoring/relay disable and endpoint changes invalidate earlier callbacks,
close connections and clear activity. Native UI keeps the existing tooltip
height and labels the header `Thinking`, without adding cloud-session rows.

`gemma4:e4b` downloaded successfully before the user-requested removal of
`gemma3:270m`. The [official E4B tag](https://ollama.com/library/gemma4:e4b)
is the requested 8B total-parameter model (effective 4B; Q4_K_M, 9.6 GB download).
The actual `/api/ps` listing and running app show 8.8 GB RAM with a 2,048-token
context and the automatically detected Gemma icon. Other downloaded models
were retained.

Verification: the full suite passed 585 tests (584 passed, one opt-in live
listing test skipped), then the 10 thinking/parser/transport tests passed
again after the Settings-menu connection. Native thinking tooltips were
rendered and checked on all four edges. The real standalone app showed both
Gemma 4 and Qwen thinking simultaneously, with their individual white arcs
and `Thinking` accessibility labels. After stream completion, both indicators
cleared. A separate Gemma request emitted 323 thinking chunks followed by
1,217 answer chunks: the real app had no thinking indicator while answer
streaming was still in progress. These bounded runs hit their token caps and
are activity checks, not successful puzzle solutions or quality benchmarks.
Ignored artifacts under `build/ollama-thinking-live` contain timing/phase
counts only, not model reasoning or replies. The replacement model and Qwen
are left loaded with a 15-minute idle expiry for inspection.

Example client connection (only this invocation changes its endpoint):

```sh
OLLAMA_HOST=http://127.0.0.1:11435 ollama run gemma4:e4b --think
```

The actual Settings window was checked with the relay enabled and `Ready` at
11435. Turning its switch off closed the listener; turning it back on restored
the forwarding connection. `OLLAMA_HOST=http://127.0.0.1:11435 ollama list`
returned the installed inventory, including Gemma 4 and excluding Gemma 3.
A final Gemma 4 arithmetic smoke check completed normally with the expected
single-digit answer. It emitted no thinking chunks for that trivial prompt,
confirming why a `think: true` request alone must not animate the ring. Unrelated
cloud-provider preference changes are filtered so they do not restart the relay
or interrupt an Ollama request.

## Generation speed and ring colors (2026-09-07)

After the user approved the speed proposal, local cells changed from RAM to
the most recent completed response's generation tok/s. The native Ollama final
response supplies `eval_count` and nanosecond `eval_duration`:
`tok/s = eval_count / (eval_duration / 1_000_000_000)`. Loading, prompt evaluation
and network chunk counts do not enter this calculation. Both native streaming
and non-streaming `/api/chat` and `/api/generate` responses work through the
relay. OpenAI-compatible responses cannot supply this metric without native
generation duration. Incomplete, failed or invalid measurements leave the
previous valid speed intact; absent history shows `— tok/s`.

`LocalModelPerformance` → relay `performances` → `NotchViewModel` snapshot
decoration → `ProviderCell`/`ProviderRing` and `RuntimeModelDetails` form the
display path. Inventory polling preserves the latest measurement. The cache
keeps at most 128 models in memory and clears on relay/monitor disable, endpoint
change or relaunch. Unloading still removes the model cell. No prompt or reply
is retained, and monitoring does not initiate generation.

The full outer ring is blue at ≥40 tok/s, green at 20–<40, yellow at 10–<20,
red below 10, and gray before measurement. These are UI speed thresholds and
do not diagnose hardware health. The inner white reasoning arc is independent.
The tooltip has six rows: last speed, textual speed band, RAM, context limit,
quantization and measurement age. Quantization was subsequently added from
`/api/ps` model `details.quantization_level` (for example `Q4_K_M` or `Q8_0`).
Missing or blank metadata displays `Unavailable`; the model tag is not used to
guess it. Shared tooltip height/hit geometry was updated together,
and accessibility includes speed and its text meaning. Settings explains the
relay address, measurements and colors.

The quantization follow-up passed 601 tests with the opt-in live test skipped.
Four-edge tooltip renders include `Q4_K_M`, `Q8_0` and missing metadata. In the
running app, the five loaded demonstration models retained separate speed cells;
Llama 3.2 1B displayed `Q8_0`, while Qwen3 0.6B, DeepSeek-R1 1.5B, Ministral 3
3B and Gemma 4 E4B displayed `Q4_K_M`, matching their live `/api/ps` metadata.

Verification: 595 tests completed with 594 passed, one opt-in live listing test
skipped and no failures. New coverage includes exact thresholds, official
metrics versus network chunks, malformed/absent metrics, both native response
modes, OpenAI limitations, per-model scoping across polls, reset behavior,
compact label bounds and all four notch edges. Real HTTP fixture responses
retain exact bytes while delivering one speed measurement for both native
modes. Four-edge notch and tooltip renders were inspected.

The newly built standalone app then forwarded a bounded Qwen 0.6B native stream
at `11435`. Ollama returned 128 generated tokens and 532,124,000 ns, or
240.5454 tok/s. The real right-edge cell showed `241 tok/s`, a blue full ring,
and an accessibility label with `240.5 tok/s`, `Very fast`, RAM and context.
This token-capped request verifies display plumbing; it is not a comparative
hardware benchmark. The actual Settings tree exposed the enabled speed/thinking
switch and `Ready` relay status. Artifacts under `build/ollama-speed-live` store
completion metrics and runtime metadata only; test evidence is in
`build/ollama-speed-complete-tests.log` and its `.xcresult`.

The user had requested background models unloaded before their own manual run.
The earlier Gemma/Qwen demo models were therefore unloaded, and this final
verification started from an empty `/api/ps` list. Its Qwen model was unloaded
after checking its expiry had not changed through user activity. The final
runtime list was empty. The updated app, Ollama server and opt-in relay remain
running, with the user's current right-edge/hover/menu-bar choices preserved.

### Independent local model animations

Local cells previously used the shared `ollama` refresh flag, so clicking one
model or discovering another pressed every loaded model's ring. Click feedback
now belongs to the unique cell ID and awaits the shared inventory request;
background polls do not press local rings. Repeated clicks on one cell coalesce,
while another cell can show its own feedback. Existing local cells retain their
order and newly detected cells append with their own entrance transition.
Snapshot updates no longer wrap the entire list in an unfolding animation.
Panel click handling uses the event's location instead of the later global
cursor position. The bar retains its existing centering and sizing behavior.

The full suite completed 606 tests: 605 passed, one opt-in live listing test was
skipped, and none failed. Coverage includes concurrent cell feedback, joining
an in-flight inventory poll, model insertion/removal order, hover identity, and
panel-event routing to the selected model on all four edges. Native four-edge
renders were inspected. In the running right-edge sidebar, Qwen, Llama and
DeepSeek were verified with real local readings. A 60 fps recording shows each
clicked ring shrinking independently to 80 pixels while both neighboring rings
remain 88 pixels wide throughout that click. A separate capture confirms a
new model's entrance without reordering or pressing existing model rings.
Evidence is under `build/independent-animation`; the full test log is
`build/ollama-independent-tests-rerun.log`. The rebuilt app is running and
the original `Show on hover` preference was restored after recording.

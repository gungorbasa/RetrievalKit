# Apple End-to-End Benchmark Contract V2: USB-Powered iPhone Amendment

Status: frozen before the first accepted physical-iPhone latency session.

V2 inherits every corpus, query, model, index, search, timing, warmup, sampling,
quality-prerequisite, thermal, memory-safety, and workload-classification rule
from `apple-end-to-end-benchmark-contract-v1.md`. It changes only the physical
iPhone power/control condition and therefore uses new iPhone workload IDs.

## Amendment

Physical-iPhone sessions run with a wired USB CoreDevice control channel and
may report battery state `charging` or `full`. Battery remains constrained to
50-90%, Low Power Mode remains off, and start thermal state must be nominal.
End thermal state must be nominal or fair; serious/critical thermal state,
memory warning, backgrounding, debugger attachment, or available Wi-Fi/cellular
network still aborts the session.

This makes unattended fresh-process collection reproducible. It does not claim
unplugged-user latency, energy consumption, battery-life behavior, or charging
thermal behavior. Published iPhone numbers must be labeled `USB-powered`.

The completed Mac V1 evidence is not relabeled as V2. It remains the unchanged
Mac comparison for the byte-identical corpus, model, index, query, and search
matrix. The combined report must show the contract distinction explicitly.

## V2 iPhone workload IDs

- `apple-e2e-10k-384d-i8-usb-powered-v2`
- `apple-e2e-50k-384d-i8-usb-powered-boundary-v2`
- `apple-e2e-100k-384d-i8-usb-powered-stress-v2`

The 10K/50K/100K interpretation remains product, qualification boundary, and
non-marketing stress respectively. V2 does not change the fewer-than-50K V1
support envelope or production-qualify Q8.

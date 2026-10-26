# GNU Radio Interop (Route 1)

Exchanges a DIFI stream between our `difi-tool` and a GNU Radio flowgraph over UDP, in
both directions, putting GNU Radio's runtime + its `network.socket_pdu` UDP block in the
path. See `docs/interop/GNURADIO-EVIDENCE.md` for results and the Route 1 vs Route 2
distinction.

## Files

| File | Purpose |
|------|---------|
| `difi_gr_common.py` | Reproduces the reference signal + float→DIFI mapping, and builds/parses DIFI packets with the DIFI Consortium `construct` definitions. |
| `gr_receive.py` | GNU Radio flowgraph: `socket_pdu(UDP_SERVER)` → parse → verify (ours→GR). |
| `gr_send.py` | GNU Radio flowgraph: build packets → `socket_pdu(UDP_CLIENT)` (GR→ours). |
| `run_interop.sh` | Drives all four directions (rust/cpp × send/receive); skips if GNU Radio absent. |

## Running

```sh
# 1. Install `construct` into the GNU Radio venv python (one-time).
#    The runner auto-detects /opt/homebrew/Cellar/gnuradio/*/libexec/venv/bin/python;
#    override with DIFI_GR_PYTHON.
<gr-python> -m pip install construct

# 2. Build both tools.
cd rust && cargo build --bin difi-tool && cd ..
cmake -S cpp -B cpp/build && cmake --build cpp/build

# 3. Run.
./tests/gnuradio/run_interop.sh
```

Interop is judged within one quantisation step (1 LSB), since cross-language float math
can map an isolated sample to the adjacent code. See the evidence doc.

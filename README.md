<img width="80" height="80" src="https://8upload.com/image/91a3bae6814b5df7/anneminer.png" alt="image">
 
# ANNE Miner: Low-Energy Proof Of Space Time Miner

[![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](LICENSE)  
[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://rust-lang.org)  
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey)]()

**ANNE Miner** is a low‑energy Proof of Space Time (PoST) miner for the ANNE network. It reads pre‑computed plots on your hard drives and submits mining intents to the network, allowing you to earn annecoin using spare disk space.

### Download the Latest Release

Head over to the [Releases](https://github.com/annemedia/anne-miner/releases) page and download the binary for your operating system:

### Quick Start with ANNE Wizard

The easiest way to set up everything (ANNE Node, ANNE hasher, and ANNE miner) is to use the **[ANNE Wizard](https://anne.media/anne-wizard-anne-installation-suite/)** - it configures the miner automatically.

### What ANNE Miner Does

- **Reads your plots** – after you run ANNE Hasher to fill your drives, the miner scans them for each new block.
- **Computes deadlines** – uses the current block challenge to calculate how long you must wait before forging a block.
- **Submits intents** – when a valid deadline is found, it broadcasts an intent to the network.
- **Supports solo and share mining** – choose your preferred tier.
- **Runs continuously** – works in the background, responding to new blocks automatically.

### Why Hard Drive Mining?

- **Low energy** – uses a fraction of the power of GPU/ASIC mining.
- **Uses existing hardware** – any computer with spare disk space can participate.
- **Quiet operation** – no loud fans, runs on Raspberry Pi or old laptops.
- **Fair distribution** – two‑tier system prevents large miners from dominating.
- **No ASIC arms race** – storage‑based, consumer drives compete evenly.

### Solo vs. Share Mining

ANNE's two‑tier mining structure gives you a choice:

- **Solo mining** – compete directly for the solo block. Every fifth block is solo‑only.
- **Share mining** – compete in a separate pool where seven winners each receive a share of the block reward. Even with small plots you can earn regularly.

You can switch at any time by editing your configuration.

### How to Start Mining (Manual Steps)

1. **Install ANNE Node and ANNE Hasher** (or use the ANNE Wizard).
2. **Plot your drives** with ANNE Hasher.
3. **Configure ANNE Miner** – edit `config.yaml` to point to your plot directories and choose your mining tier.
4. **Run ANNE Miner** – it will start scanning and submitting intents.
5. **Earn annecoin** – rewards are deposited directly into your ANNE wallet.

## Features
  
- **ANNE APIs** – `SubmitNonce` and `GetMinerInfo` pass ANNE‑specific information into the miner.
- **Energy optimizations** – improved efficiency, safeguards, non-live mode scanning interruption, and fork prevention.
- **Stop codes** – annode alters miner behavior with new stop codes.
- **Mining mode awareness** – miner tracks ANNE MINING MODE (grace period) and pauses or stops scanning when not in LIVE mode.
- **Protocol compliance** – miner respects SOLO vs SHARE rules.
- **Legacy cleanup** – removed variables not relevant to ANNE (cumulative difficulty, commitment, etc.).
- **GPU/iGPU support** – set `gpu_mem_mapping: true` for iGPU, else `false`.
- **Automatic SIMD detection** – universal executables auto‑select best CPU instructions at runtime.
- **Terminal detection** – retains ANNE‑branded terminal‑as‑a‑GUI when opened from file browser.
- **Daemon mode** – use `--daemon` for background operation (bash/bat/systemd).
- **Isolated scanning** – only scans plots belonging to the NID defined in config, preventing cross‑account interference.
- **Rust upgrade** – compiled with latest Rust stable.
- **Platform support** – Linux (GLIBC 2.35+), macOS Sierra+ (Intel/Silicon), Windows 10+, Termux
- **Multi‑architecture** – x86 32/64‑bit, ARM, AArch64.
- **Direct I/O** – efficient disk access.
- **SIMD optimizations** – AVX512F, AVX2, AVX, SSE, NEON.
- **OpenCL support** – optional GPU acceleration.

## Usage

Run the miner with:

```bash
anne-miner --help
```

Example:

### Running the Miner

```bash
# Normal foreground mode
./anne-miner --config /path/to/config.yaml

# Daemon mode (background, no console)
./anne-miner --daemon --config /path/to/config.yaml
```

*NOTE:* The config parameter doesn't have to be specified if config.yaml is localed in the same directory.

## Configuration

The miner reads settings from a **config.yaml** file. Below is a minimal example with important security notes.

### Security: Store Miner Keys in Annode Properties, Not Miner's Config

For security, **do not** store your miner’s seed phrase directly in `config.yaml`. Instead, configure the seed in your **encrypted annode `node.properties`**, then the miner only needs your account ID. The `account_id_to_secret_phrase` mapping in `config.yaml` simply indicates the mining mode (`-SHARE` or `-SOLO`). The actual secret phrase is retrieved by the annode from it's own `node.properties`.

**In node.properties (annode):**

```properties
# Enable share mining for this account
AllowOtherShareMiners = true
ShareMiningPassphrases = yourshareminerseed(s);

# Enable solo mining for this account
AllowOtherSoloMiners = true
SoloMiningPassphrases = yoursolominerseed(s);
```

**In config.yaml (miner):**

```yaml
# Mapping: account ID -> mode (-SHARE or -SOLO)
account_id_to_secret_phrase:
  "12345678901234567890": "-SHARE"   # or "-SOLO"

# Plot directories (recommended naming of directories with the account ID)
plot_dirs:
  - "/home/user/12345678901234567890"
  - "/mnt/mountpoint/12345678901234567890"

# Annode API URL (must match anne.nodeURI in node.properties)
url: 'http://localhost:9116'
```

### Explanation of Settings

| Setting | Description |
|---------|-------------|
| `account_id_to_secret_phrase` | Maps your account ID to `-SHARE` or `-SOLO`. The actual seed is stored in annode’s encrypted properties. |
| `plot_dirs` | List of directories containing plots. Each should be named exactly the account ID (e.g., `12345678901234567890`). |
| `url` | Annode API endpoint. Must match the `anne.nodeURI` in your node.properties. |
| `hdd_reader_thread_count` | 0 = auto (one thread per disk). |
| `cpu_threads` / `gpu_threads` | Set to 0 to disable. Adjust based on your hardware. |
| `gpu_mem_mapping` | Set `true` for integrated GPU (iGPU), `false` for dedicated GPU. |
| `console_log_level` / `logfile_log_level` | Logging verbosity. |
| `benchmark_only` | Use `"I/O"` or `"XPU"` to test disk/GPU performance without mining. |

For a complete template with all options, see [config.yaml.example-linux](config.yaml.example-linux) or [config.yaml.example-macos](config.yaml.example-macos) or [config.yaml.example-windows](config.yaml.example-windows) in the repository.

## Prerequisites (for developers)

To build from source you need:

- **Rust** stable toolchain – [install](https://www.rust-lang.org/tools/install).
- **Optional:** OpenCL libraries for GPU mining.

## Building from Source

Clone the repository:

```bash
git clone https://github.com/annemedia/anne-miner.git
cd anne-miner
```

Build:

```bash
# CPU Only
cargo build --release

# OpenCL (GPU) support
cargo build --release --features=opencl
```

The binary will be placed in `target/release/anne-miner`.

## Contributing

We welcome contributions! You can help by:

- Reporting bugs or suggesting features via GitHub Issues
- Submitting pull requests for bug fixes or improvements
- Improving documentation
- Adding translations

For discussions, join our community at [ANNE Forum](https://annetalk.org).

## Limitations

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

## Acknowledgements

ANNE Miner builds on the work of many open‑source contributors. The codebase originated with the PoC Consortium (Burstcoin), was later maintained by the Signum Network, and subsequently adapted by the ANNE Network. This version, with the features listed above, reflects a collaborative effort between the ANNE Network and ANNE Media, with significant enhancements, optimizations, and security improvements tailored to the ANNE ecosystem, and is now maintained by ANNE Media.

- Signum Network Miner – [signum-miner](https://github.com/signum-network/signum-miner)
- ANNE Official – [anne.network](https://anne.network)
- ANNE Media – [anne.media](https://anne.media)
- The ANNE community – [annetalk.org](https://annetalk.org)
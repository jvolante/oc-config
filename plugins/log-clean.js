// Cleans bash tool output before it reaches the model:
//
//   1. CR-based progress collapse: simulates terminal CR-overwrite so
//      "Receiving objects:   0%\r...100%" collapses to one line.
//   2. Nix fetch noise suppression: collapses "these N paths will be fetched"
//      blocks and "copying path '...'" lines to a single summary line.
//   3. Secret redaction: replaces credentials with stable [SECRET:type:N]
//      markers. N is assigned per unique value and is stable across the
//      entire opencode session (module-scope state survives all tool calls).
//
// Redacted patterns: PEM/certificate blocks, Circle-Token, Bearer tokens,
// Basic auth, AWS access key IDs, GitHub tokens (ghp_/ghs_), GitLab PATs
// (glpat-), SSH public key blobs (known_hosts format), env var assignments
// whose name contains _KEY/_TOKEN/_SECRET/_PASSWORD/_PASSWD/_CREDENTIAL.

// --- Session-scoped secret registry ----------------------------------------
// These live at module scope so the same value gets the same ID across every
// bash call within one opencode session. Resets on process restart, which is
// correct — IDs only need to be stable within a session.
let _counter = 0
const _ids = new Map()

function secretId(val) {
  if (_ids.has(val)) return _ids.get(val)
  const id = ++_counter
  _ids.set(val, id)
  return id
}

// --- Progress collapse -------------------------------------------------------
// Normalize \r\n → \n first so Windows line endings aren't mistaken for
// progress overwrites, then simulate terminal CR overwrite by keeping only
// the text after the last bare \r on each line.
function collapseProgress(text) {
  // Step 1: normalize CRLF
  const normalized = text.replace(/\r\n/g, "\n")
  // Step 2: per line, keep only the final overwrite state
  return normalized
    .split("\n")
    .map((line) => {
      const last = line.lastIndexOf("\r")
      return last === -1 ? line : line.slice(last + 1)
    })
    .join("\n")
}

// --- Nix fetch noise suppression ---------------------------------------------
// Collapses two kinds of nix verbosity:
//   "these N paths will be fetched (X MiB download, Y MiB unpacked):"
//   followed by N "  /nix/store/..." lines  →  one "[nix: fetching N paths]" line
//
//   "copying path '/nix/store/...'" messages → dropped, including when
//   concatenated with adjacent tool output
function suppressNixFetchNoise(text) {
  const lines = text.split("\n")
  const out = []
  let fetchBlock = 0
  const copyingPath = /copying path '\/nix\/store\/[^']+' from '[^']+'\.\.\./g

  for (const line of lines) {
    const fetchHeader = line.match(/^these (\d+) paths will be fetched/)
    if (fetchHeader) {
      out.push(`[nix: fetching ${fetchHeader[1]} paths]`)
      fetchBlock = parseInt(fetchHeader[1], 10)
      continue
    }
    if (fetchBlock > 0 && /^\s+\/nix\/store\//.test(line)) { fetchBlock--; continue }
    fetchBlock = 0
    const cleaned = line.replace(copyingPath, "")
    if (cleaned.trim() || line === cleaned) out.push(cleaned)
  }
  return out.join("\n")
}

function suppressExecutingCudaToolKit(text) {
  const lines = text.split("\n")
  let out = []
  for (const rawLine of lines) {
    if (!/Executing setupCUDAToolkitCompilers/.test(rawLine)) {
      out.push(rawLine)
    }
  }
  return out.join("\n")
}

// --- Nix dep build log suppression -------------------------------------------
// With `nix build -L`, dependency build output is prefixed "depname> " or
// "depname (post)> ". These lines are buffered per dep and discarded on
// success (dep lines simply stop). On failure, the buffer is flushed
// immediately before the error line so the relevant log is preserved.
//
// Nix's own "Last N log lines:" replay block (indented "> ") is also dropped
// since the real lines are already in the buffer.
function suppressNixDepLogs(text) {
  const lines = text.split("\n")
  const out = []
  const depBuffers = new Map()
  let inReplayBlock = false

  for (const line of lines) {
    // dep build log line: "depname> ..." or "depname (post)> ..."
    const depLog = line.match(/^([A-Za-z0-9_.+-]+(?:\s+\(post\))?)\> /)
    if (depLog) {
      const dep = depLog[1]
      if (!depBuffers.has(dep)) depBuffers.set(dep, [])
      depBuffers.get(dep).push(line)
      continue
    }

    // nix's "Last N log lines:" replay block — drop it entirely
    if (/^\s+Last \d+ log lines:/.test(line)) { inReplayBlock = true; continue }
    if (inReplayBlock) {
      if (/^\s+> /.test(line) || /^\s+For full logs, run:/.test(line) || /^\s+nix log /.test(line)) continue
      inReplayBlock = false
      // fall through — first non-replay line
    }

    // dep failure — flush that dep's buffer before the error line
    // Handles both: "error: Cannot build '...-DEPNAME-VER.drv'."
    //           and: "error: builder for '...-DEPNAME-VER.drv' failed"
    const failMatch =
      line.match(/^error: Cannot build '\/nix\/store\/[^-]+-(.+)\.drv'/) ||
      line.match(/^error: builder for '\/nix\/store\/[^-]+-(.+)\.drv' failed/)
    if (failMatch) {
      const drvName = failMatch[1]
      for (const [dep, buf] of depBuffers) {
        if (drvName.startsWith(dep) || drvName.includes(`-${dep}-`) || drvName.includes(`-${dep}`)) {
          out.push(...buf)
          depBuffers.delete(dep)
          break
        }
      }
    }

    out.push(line)
  }

  return out.join("\n")
}

// --- Secret redaction --------------------------------------------------------
function redactSecrets(text) {
  const lines = text.split("\n")
  const out = []
  let inPem = false
  let pemKey = null

  for (const rawLine of lines) {
    let line = rawLine

    // PEM block handling (stateful)
    if (/-----BEGIN ([A-Z ]*KEY|CERTIFICATE|ENCRYPTED PRIVATE KEY)-----/.test(line)) {
      pemKey = line.trim()
      out.push(`[SECRET:pem-key:${secretId("pem:" + pemKey)}]`)
      inPem = true
      continue
    }
    if (inPem) {
      if (/-----END /.test(line)) inPem = false
      continue
    }

    // Circle-Token: <value>
    line = line.replace(/Circle-Token:\s*([^\s"'\\]+)/, (_, val) => {
      return `Circle-Token: [SECRET:circle-token:${secretId(val)}]`
    })

    // Authorization: Bearer <token>
    line = line.replace(/Authorization:\s*Bearer\s+([^\s"'\\]+)/, (_, val) => {
      return `Authorization: Bearer [SECRET:bearer-token:${secretId(val)}]`
    })

    // Authorization: Basic <value>
    line = line.replace(/Authorization:\s*Basic\s+([^\s"'\\]+)/, (_, val) => {
      return `Authorization: Basic [SECRET:basic-auth:${secretId(val)}]`
    })

    // AWS access key IDs: AKIA + 16 uppercase alphanumerics
    line = line.replace(/AKIA[A-Z0-9]{16}/g, (val) => {
      return `[SECRET:aws-key:${secretId(val)}]`
    })

    // GitHub tokens: ghp_ or ghs_ followed by 36+ alphanumerics
    line = line.replace(/gh[ps]_[A-Za-z0-9]{36,}/g, (val) => {
      return `[SECRET:github-token:${secretId(val)}]`
    })

    // GitLab PATs: glpat- followed by exactly 20 alphanumerics/hyphens
    line = line.replace(/glpat-[A-Za-z0-9-]{20}/g, (val) => {
      return `[SECRET:gitlab-token:${secretId(val)}]`
    })

    // SSH public key blobs in known_hosts format:
    // <host> <key-type> AAAA<base64blob>
    line = line.replace(
      /^([^\s]+\s+(?:ssh-rsa|ssh-ed25519|ecdsa-sha2-nistp(?:256|384|521))\s+)(AAAA[A-Za-z0-9+/]+=*)/,
      (_, prefix, blob) => {
        const keyType = (prefix.match(/(ssh-rsa|ssh-ed25519|ecdsa-sha2-nistp(?:256|384|521))/) || ["", "unknown"])[1]
        return `${prefix}[PUBLIC-KEY:${keyType}:${secretId(blob)}]`
      },
    )

    // Env var assignments with secret-sounding names:
    // export FOO_TOKEN=value  or  FOO_SECRET=value
    line = line.replace(
      /(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*(?:_KEY|_TOKEN|_SECRET|_PASSWORD|_PASSWD|_CREDENTIAL))\s*=\s*([^\s"';\\]+)/gi,
      (match, name, val) => {
        if (/^\[SECRET:/.test(val)) return match
        return `${match.slice(0, match.indexOf(val))}[SECRET:env-secret:${secretId(val)}]`
      },
    )

    out.push(line)
  }

  return out.join("\n")
}

// --- Pipeline ----------------------------------------------------------------
const cleaners = [
  suppressExecutingCudaToolKit,
  collapseProgress,
  suppressNixFetchNoise,
  redactSecrets,
]

function clean(text) {
  return cleaners.reduce((t, fn) => fn(t), text)
}

// --- Plugin entry point ------------------------------------------------------
export const LogClean = async () => {
  return {
    "tool.execute.after": (input, output) => {
      if (input.tool !== "bash") return
      if (typeof output.output !== "string") return
      output.output = clean(output.output)
    },
  }
}

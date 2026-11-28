import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";

const args = process.argv.slice(2);
const env = { ...process.env };

if (args[0] === "build") {
  const paths = new Map();
  for (const [directory, replacement] of [
    [homedir(), "/build/host"],
    [env.CARGO_HOME, "/build/cargo"],
    [env.RUSTUP_HOME, "/build/rustup"],
    [env.CARGO_TARGET_DIR, "/build/target"],
    [process.cwd(), "/build/holdingscope"],
  ]) {
    if (!directory) continue;
    const absolute = resolve(directory);
    let canonical = absolute;
    try {
      canonical = realpathSync(absolute);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    for (const path of [absolute, canonical]) {
      paths.set(path, replacement);
      paths.set(path.replaceAll("\\", "/"), replacement);
    }
  }

  // Cargo gives encoded flags precedence. Keep explicit caller flags and support paths with spaces.
  const flags = env.CARGO_ENCODED_RUSTFLAGS !== undefined
    ? env.CARGO_ENCODED_RUSTFLAGS.split("\x1f").filter(Boolean)
    : (env.RUSTFLAGS ?? "").split(/[\t\n\v\f\r ]+/).filter(Boolean);
  const prefixes = [...paths].sort(([left], [right]) => left.length - right.length);
  for (const [path, replacement] of prefixes) {
    flags.push("--remap-path-prefix", `${path}=${replacement}`);
  }
  env.CARGO_ENCODED_RUSTFLAGS = flags.join("\x1f");
}

const require = createRequire(import.meta.url);
const cliManifest = require.resolve("@tauri-apps/cli/package.json");
const cli = resolve(dirname(cliManifest), require(cliManifest).bin.tauri);
const result = spawnSync(process.execPath, [cli, ...args], { env, stdio: "inherit" });
if (result.error) throw result.error;
if (result.signal) process.kill(process.pid, result.signal);
process.exitCode = result.status ?? 1;

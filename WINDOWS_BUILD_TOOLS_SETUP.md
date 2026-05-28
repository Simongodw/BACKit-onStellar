# Setup Guide: Fixing Rust Build Issues on Windows

## Problem

Your Rust installation is missing the required C++ build tools (linker) needed to compile Rust code on Windows. This is a common issue when Rust is installed without the necessary development environment.

## Solution

You have two options:

### Option 1: Install Visual Studio Build Tools (RECOMMENDED)

1. **Download Visual Studio Build Tools**
   - Go to: https://visualstudio.microsoft.com/downloads/
   - Click "Download Build Tools for Visual Studio"

2. **Run the Installer**
   - Execute the downloaded installer
   - Select "Desktop development with C++"
   - Click "Install"

3. **Restart your terminal/VS Code** after installation completes

4. **Verify Installation**
   ```powershell
   link.exe /?
   ```
   If this shows the linker help, you're good to go.

5. **Test the Build**
   ```bash
   cd packages\contracts\call_registry
   cargo test --lib fuzz
   ```

### Option 2: Install MinGW (Alternative)

If you prefer the GNU toolchain:

1. **Download MinGW**
   - Go to: https://www.mingw-w64.org/downloads/
   - Download the installer

2. **Install MinGW**
   - During installation, ensure you select x86_64 architecture
   - Note the installation path (e.g., C:\mingw-w64\x86_64-8.1.0-win32-seh-rt_v6-rev0)

3. **Add to PATH**
   - Add `C:\mingw-w64\x86_64-8.1.0-win32-seh-rt_v6-rev0\bin` to your system PATH

4. **Restart your terminal**

5. **Verify Installation**
   ```powershell
   gcc --version
   ```

6. **Set as Default Toolchain**
   ```bash
   rustup default stable-x86_64-pc-windows-gnu
   ```

7. **Test the Build**
   ```bash
   cd packages\contracts\call_registry
   cargo clean
   cargo test --lib fuzz
   ```

## Verifying the Fix

After installing either option above, verify everything works:

```bash
# Navigate to the contracts directory
cd packages\contracts\call_registry

# Clean build cache
cargo clean

# Run the fuzz tests
cargo test --lib fuzz
```

You should see output like:
```
running 12 tests

test fuzz::fuzz_extreme_timestamps ... ok
test fuzz::fuzz_individual_stake_tracking ... ok
test fuzz::fuzz_multiple_stakers_multiple_calls ... ok
test fuzz::fuzz_multiple_stakers_single_call ... ok
test fuzz::fuzz_no_overflow_with_large_amounts ... ok
test fuzz::fuzz_negative_stake_fails ... ok
test fuzz::fuzz_negative_stake_min_fails ... ok
test fuzz::fuzz_same_staker_multiple_stakes ... ok
test fuzz::fuzz_stake_invariant_total_equals_sum ... ok
test fuzz::fuzz_stress_test_high_volume ... ok
test fuzz::fuzz_varied_call_durations ... ok
test fuzz::fuzz_zero_stake_fails ... ok

test result: ok. 12 passed
```

## Troubleshooting

### Error: "Cannot find mspdbcore.dll"
- You need **Visual Studio Build Tools**, not just the SDK
- Follow Option 1 above

### Error: "gcc not found" (with GNU toolchain)
- MinGW is not properly installed or not in PATH
- Reinstall MinGW and add it to your system PATH

### Still getting "link.exe not found"
- Restart your entire computer (not just terminal)
- Re-run `rustup default stable-x86_64-pc-windows-msvc` if using MSVC
- Or `rustup default stable-x86_64-pc-windows-gnu` if using GNU

## What You're Installing

These build tools are required because Soroban test code needs to compile Rust code that runs on the host machine (tests themselves), while the contracts compile to WebAssembly.

- **MSVC**: Microsoft C++ compiler and linker (works great with VS Code and Rust)
- **MinGW**: GNU compiler collection for Windows (lighter weight)

Either one will work fine. MSVC is generally recommended on Windows.

## Need More Help?

If you're still having issues after installing the build tools:

1. Verify the linker is installed:
   - MSVC: `link.exe /?`
   - MinGW: `gcc --version`

2. Try a fresh cargo build:
   ```bash
   cargo clean
   cargo build --lib
   ```

3. Check Rust installation:
   ```bash
   rustup show
   ```
   Should show your active toolchain and installed components

4. Update Rust:
   ```bash
   rustup update
   ```

---

**Once installed, you can run the fuzz tests with:**
```bash
cd packages/contracts/call_registry
cargo test --lib fuzz
```

This will run all 12 fuzz tests with 890+ total randomized iterations, verifying the call_registry staking logic invariants.

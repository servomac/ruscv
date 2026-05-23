# ---- FreeRTOS RISC-V QEMU virt demo ----------------------------------------
# Targets here clone the official FreeRTOS repo (shallow), build the
# RISC-V-Qemu-virt_GCC demo, and run the resulting ELF under ruscv.
#
# The demo targets the same board profile as our emulator (QEMU virt):
#   CLINT at 0x02000000, UART 16550 at 0x10000000, DRAM at 0x80000000.
#
# The demo is patched at build time to use -march=rv32i_zicsr so it only
# emits instructions our emulator supports (no atomics, no compressed).
#
# Requires: sudo apt install gcc-riscv64-unknown-elf
FREERTOS_SRC  := freertos-src
FREERTOS_REPO := https://github.com/FreeRTOS/FreeRTOS.git
FREERTOS_DEMO := $(FREERTOS_SRC)/FreeRTOS/Demo/RISC-V-Qemu-virt_GCC
FREERTOS_ELF  := $(FREERTOS_DEMO)/build/RTOSDemo.axf

.PHONY: freertos-get freertos-build freertos-run freertos-clean

# Clone FreeRTOS with its kernel submodule (shallow, depth=1).
# Skipped if freertos-src already exists.
freertos-get:
	@if [ ! -d $(FREERTOS_SRC) ]; then \
		echo "Cloning FreeRTOS (shallow clone, may take a moment)..."; \
		git clone --depth=1 --recurse-submodules --shallow-submodules \
			$(FREERTOS_REPO) $(FREERTOS_SRC); \
	fi

# Build the demo for rv32i_zicsr — the architecture our emulator supports.
# The FreeRTOS demo Makefile defaults to rv32imac; we patch its Makefile to
# strip the M/A/C extensions before building. The sed is idempotent.
# picolibc provides the C standard headers for bare-metal RISC-V.
# Install both with: sudo apt install gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf
freertos-build: freertos-get
	@command -v riscv64-unknown-elf-gcc >/dev/null 2>&1 || { \
		echo "Error: riscv64-unknown-elf-gcc not found."; \
		echo "Install with: sudo apt install gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf"; \
		exit 1; \
	}
	@dpkg -s picolibc-riscv64-unknown-elf >/dev/null 2>&1 || { \
		echo "Error: picolibc-riscv64-unknown-elf not found (needed for C headers)."; \
		echo "Install with: sudo apt install picolibc-riscv64-unknown-elf"; \
		exit 1; \
	}
	sed -i 's/rv32imac/rv32i/g' $(FREERTOS_DEMO)/Makefile
	$(MAKE) -C $(FREERTOS_DEMO) CROSS=riscv64-unknown-elf- PICOLIBC=1

# Run the FreeRTOS ELF under ruscv. UART output is printed to stdout in
# real time. The run ends when a StepError occurs or max_steps is reached.
freertos-run: freertos-build
	cargo run --release -- --elf $(FREERTOS_ELF)

# Remove the FreeRTOS build artefacts (keeps the cloned source).
freertos-clean:
	-$(MAKE) -C $(FREERTOS_DEMO) clean

# ---- rv32ui test suite ------------------------------------------------------
TESTS_DIR    := tests/rv32ui
TESTS_URL    := https://raw.githubusercontent.com/wokwi/riscv-tests-precompiled/master/isa
RV32UI_TESTS := add addi and andi auipc beq bge bgeu blt bltu bne \
                fence_i jal jalr lb lbu lh lhu lui lw ma_data or ori \
                sb sh simple sll slli slt slti sltiu sltu sra srai srl \
                srli sub sw xor xori

.PHONY: assemble dump download-tests run-tests freertos-get freertos-build freertos-run freertos-clean

assemble:
	riscv64-linux-gnu-as -march=rv32i -mabi=ilp32 -o test.o test.s
dump:
	riscv64-linux-gnu-objdump -d -M no-aliases test.o

# Download pre-compiled rv32ui-p ELF binaries from wokwi/riscv-tests-precompiled.
# Only fetches files that are not already present.
download-tests:
	mkdir -p $(TESTS_DIR)
	$(foreach t,$(RV32UI_TESTS), \
		test -f $(TESTS_DIR)/rv32ui-p-$(t) || \
		curl -sSL -o $(TESTS_DIR)/rv32ui-p-$(t) $(TESTS_URL)/rv32ui-p-$(t);)

# Build in release mode, then run every rv32ui-p test and report results.
# Exits with a non-zero status if any test fails.
run-tests: download-tests
	cargo build --release -q
	@pass=0; fail=0; \
	for t in $(RV32UI_TESTS); do \
		out=$$(./target/release/ruscv --elf $(TESTS_DIR)/rv32ui-p-$$t 2>&1); \
		if [ "$$out" = "PASS" ]; then \
			pass=$$((pass+1)); \
			printf "PASS  rv32ui-p-%s\n" "$$t"; \
		else \
			fail=$$((fail+1)); \
			printf "FAIL  rv32ui-p-%-10s  %s\n" "$$t" "$$out"; \
		fi; \
	done; \
	echo ""; \
	echo "$$pass passed, $$fail failed out of $$((pass+fail))"; \
	[ $$fail -eq 0 ]

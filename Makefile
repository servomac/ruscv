TESTS_DIR    := tests/rv32ui
TESTS_URL    := https://raw.githubusercontent.com/wokwi/riscv-tests-precompiled/master/isa
RV32UI_TESTS := add addi and andi auipc beq bge bgeu blt bltu bne \
                fence_i jal jalr lb lbu lh lhu lui lw ma_data or ori \
                sb sh simple sll slli slt slti sltiu sltu sra srai srl \
                srli sub sw xor xori

.PHONY: assemble dump download-tests run-tests

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

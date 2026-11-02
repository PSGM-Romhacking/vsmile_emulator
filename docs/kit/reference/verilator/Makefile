# EXAMPLE from V-Bear (Funtech Super A'Can, MiSTer core simulated with Verilator). It is a
# starting point: names, paths and the system's own details (BIOS loading, cartridge, video
# size, RAM port) must be adapted. See docs/kit-docs.md, "Validation with a reference".

# The MiSTer core of the Super A'Can simulated with Verilator (no window), to compare
# V-Bear with the hardware described in SystemVerilog.
#   make -C reference        builds reference/acan-ref
# MISTER_RTL points at the SuperACan_MiSTer sources (default: the local copy in _referencias).

MISTER ?= $(abspath ../_referencias/mister-59ba275)
RTL_SV := $(MISTER)/rtl/6502/mos6502_pkg.sv \
          $(filter-out $(MISTER)/rtl/6502/mos6502_pkg.sv, \
            $(shell find $(MISTER)/rtl/6502 $(MISTER)/rtl/board $(MISTER)/rtl/cart $(MISTER)/rtl/um6618 \
                         $(MISTER)/rtl/um6619 $(MISTER)/rtl/fx68k $(MISTER)/rtl/mem $(MISTER)/rtl/ss \
                         $(MISTER)/rtl/cheats -name '*.sv' | sort)) \
          $(MISTER)/rtl/mem/cache_ram.v

acan-ref: sim_top.sv ref_main.cpp sim.h
	verilator --cc --exe --build -j $(shell nproc) --top-module sim_top \
	  --default-language 1800-2017 -O3 --x-assign fast --x-initial fast --noassert \
	  --timescale 1ns/1ps \
	  -Wno-fatal -Wno-lint -Wno-style -Wno-BLKANDNBLK -Wno-TIMESCALEMOD -Wno-MULTIDRIVEN -Wno-COMBDLY \
	  -I$(MISTER)/rtl/fx68k -I$(MISTER)/rtl/ss -I$(MISTER)/rtl/6502 \
	  -CFLAGS "-O3 -std=c++17 -Wno-error -I$(CURDIR)" \
	  --Mdir obj_dir -o acan-ref $(MISTER)/rtl/ss/ss_map.svh $(RTL_SV) sim_top.sv ref_main.cpp
	install -m 755 obj_dir/acan-ref ./acan-ref
	# fx68k reads its microcode ROMs from the current directory
	cp $(MISTER)/rtl/fx68k/microrom.mem $(MISTER)/rtl/fx68k/nanorom.mem .

clean:
	rm -rf obj_dir acan-ref microrom.mem nanorom.mem

.PHONY: clean

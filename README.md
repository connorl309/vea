# VEA = Variably Encoded Architecture

## [![CI](https://github.com/connorl309/chip/actions/workflows/ci.yml/badge.svg)](https://github.com/connorl309/chip/actions/workflows/ci.yml)

## What is Vea?
Vea, pronounced `vay-uh`, is an acronym of the phrase variably encoded architecture. It is my designation for this hodge-podge of stuff I have tossed together in the hopes of making an interesting simulator, exploring architectural design and simulation choices, as well as to work on my RTL and PCB design skills (eventually...)

Slightly more formally, Vea is a 64-bit big endian architecture that is variably encoded. Instructions can be anywhere from 2 to 13 bytes in size. It is a load-store architecture.

# AI disclosure!!
This is my second time actively using AI outside of work on a programming project. I used Claude Code exclusively on Sonnet, varying the effort and thinking settings based on the goal (i.e. writing UI code with ratatui source available = medium effort, no thinking; testgen xhigh, thinking; etc.). Claude was allowed to write code! Claude wrote the bulk of the assembler parsing and emission logic; all of the TUI/CLI code; writing comments and test cases.

## Project goals
- [x] Working instruction simulator for Vea
- [x] Working pipelined simulator for Vea
- [ ] Verilog model for a Vea core
- [ ] Functional test suite for Verilog core
- [ ] Circuit board dev-board design for an FPGA that can fit a Vea core onboard

### Stretch goals
- [ ] icache / dcache implementations depending on PCB memory interface
- [ ] some compiler backend for my assembly language (LLVM? would be nice to compile some actual programs and run them...)
- [ ] more tests
- [ ] better tooling and debug

## What's included

- Rust simulators
  - see `vea_sim/src/onestep` for the instruction-level simulator
  - see `vea_sim/src/nstep` for the slightly-more-accurate pipelined simulator
- RTL
  - see `core/` for this
- (Eventually!) circuit boards
  - (eventually!) see `hardware/` for this

## Inaccuracies and/or inefficiencies

1. No caches (yet; my implementing them depends on what kind of memory interface I end up using for devboards)
2. The current pipeline is minimally designed and does not have any possibility of hazards or mispredict penalties. This is likely to change as I continue developing this project and make more and more code/design changes.
3. Simulators assume each pipe stage takes a cycle and that memory accesses are free
4. No exceptions built into the architecture (yet)
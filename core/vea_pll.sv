//! Clock generator for the Vea board.
//!
//! The board oscillator gives 25 MHz. The core runs faster than that, so this module
//! multiplies the oscillator frequency with one EHXPLLL block. The LFE5U-12F has two of
//! these blocks.
//!
//! The core clock is 75 MHz. Place and route reports near 90 MHz for this design at
//! speed grade 6. The lower board frequency keeps margin for temperature and voltage.
//!
//! Project Trellis ecppll calculated the divider values below. To change a frequency,
//! run the tool again and copy the new values. Do not edit the dividers by hand:
//!   ecppll -i 25 -o 75 -n vea_pll --clkin_name clk_in --clkout0_name clk_core
//!
//! The FREQUENCY_PIN attributes give nextpnr the frequency of each clock. Without them
//! the timing report covers the input pin only, and the core paths stay unconstrained.

module vea_pll (
  //! The board oscillator, 25 MHz.
  input  logic clk_in,

  //! The core clock, 75 MHz.
  output logic clk_core,

  //! High after the PLL holds the target frequency. The core clock is not stable before
  //! this signal goes high, so the board top holds the core in reset until then.
  output logic locked
);

  (* FREQUENCY_PIN_CLKI="25" *)
  (* FREQUENCY_PIN_CLKOP="75" *)
  (* ICP_CURRENT="12" *) (* LPF_RESISTOR="8" *)
  (* MFG_ENABLE_FILTEROPAMP="1" *) (* MFG_GMCREF_SEL="2" *)
  EHXPLLL #(
    .PLLRST_ENA      ("DISABLED"),
    .INTFB_WAKE      ("DISABLED"),
    .STDBY_ENABLE    ("DISABLED"),
    .DPHASE_SOURCE   ("DISABLED"),
    .OUTDIVIDER_MUXA ("DIVA"),
    .OUTDIVIDER_MUXB ("DIVB"),
    .OUTDIVIDER_MUXC ("DIVC"),
    .OUTDIVIDER_MUXD ("DIVD"),
    .CLKI_DIV        (1),
    .CLKOP_ENABLE    ("ENABLED"),
    .CLKOP_DIV       (8),
    .CLKOP_CPHASE    (4),
    .CLKOP_FPHASE    (0),
    .FEEDBK_PATH     ("CLKOP"),
    .CLKFB_DIV       (3)
  ) u_pll (
    .RST          (1'b0),
    .STDBY        (1'b0),
    .CLKI         (clk_in),
    .CLKOP        (clk_core),
    .CLKFB        (clk_core),
    .LOCK         (locked),
    // The design uses one output only. The other outputs stay open.
    .CLKOS        (),
    .CLKOS2       (),
    .CLKOS3       (),
    .CLKINTFB     (),
    .PHASESEL0    (1'b0),
    .PHASESEL1    (1'b0),
    .PHASEDIR     (1'b1),
    .PHASESTEP    (1'b1),
    .PHASELOADREG (1'b1),
    .PLLWAKESYNC  (1'b0),
    .ENCLKOP      (1'b0),
    .ENCLKOS      (),
    .ENCLKOS2     (),
    .ENCLKOS3     (),
    .INTLOCK      (),
    .REFCLK       ()
  );

endmodule

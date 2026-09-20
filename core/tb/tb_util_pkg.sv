//! Check helpers that all testbenches share.
//! A failed check does not stop the run. This way one run shows every failure.

package tb_util_pkg;

  int checks = 0;
  int errors = 0;

  task automatic check(input string what, input bit ok);
    checks++;
    if (!ok)
    begin
      errors++;
      $display("FAIL: %s", what);
    end
  endtask

  //! The compare uses !==, so a value with X or Z bits fails.
  task automatic check_eq(input string what, input logic [127:0] got, input logic [127:0] expected);
    checks++;
    if (got !== expected)
    begin
      errors++;
      $display("FAIL: %s: got %h, expected %h", what, got, expected);
    end
  endtask

  //! The nonzero exit code lets make and CI see the failure.
  task automatic report(input string name);
    $display("%s: %0d checks, %0d failures", name, checks, errors);
    if (errors != 0)
      $fatal(1, "%s: FAIL", name);
    else
      $display("%s: PASS", name);
  endtask

endpackage

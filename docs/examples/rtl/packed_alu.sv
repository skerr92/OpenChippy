// Import with Yosys + slang, top module packed_alu.
package alu_types;
  typedef struct packed {
    logic [3:0] sum;
    logic carry;
  } result_t;
endpackage

module packed_alu (
  input logic [3:0] a, b,
  output alu_types::result_t result
);
  always_comb
    {result.carry, result.sum} = {1'b0, a} + {1'b0, b};
endmodule

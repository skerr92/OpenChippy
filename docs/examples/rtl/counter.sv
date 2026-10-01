// Import with Yosys, top module counter.
module counter #(
  parameter WIDTH = 8
) (
  input wire tick,
  input wire reset,
  input wire enable,
  output reg [WIDTH-1:0] count
);
  always @(posedge tick or posedge reset)
    if (reset)
      count <= 0;
    else if (enable)
      count <= count + 1'b1;
endmodule

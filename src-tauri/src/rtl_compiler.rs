//! Compiler boundary: source is elaborated by Yosys, then lowered from its typed
//! JSON netlist into the same scalar logic used by OpenChippy's simulator.
use crate::rtl::*;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frontend {
    #[default]
    Auto,
    Native,
    Yosys,
    Slang,
}

pub fn import(
    source: &str,
    top: Option<&str>,
    frontend: Frontend,
    executable: Option<&str>,
) -> Result<RtlModule, String> {
    if source.len() > 16 * 1024 * 1024 {
        return Err("Verilog source exceeds the 16 MiB import limit".into());
    }
    if matches!(frontend, Frontend::Native) {
        return parse_structural_verilog(source);
    }
    let executable = executable
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(find_yosys);
    if matches!(frontend, Frontend::Auto) && executable.is_none() {
        return parse_structural_verilog(source).map_err(|error| format!("{error}\nFor broader RTL support, install Yosys and set OPENCHIPPY_YOSYS to its executable, or add yosys to PATH."));
    }
    let executable = executable.ok_or(
        "Yosys was not found. Set OPENCHIPPY_YOSYS to its executable or add yosys to PATH.",
    )?;
    compile(source, top, frontend, executable)
}

fn find_yosys() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OPENCHIPPY_YOSYS") {
        return Some(path.into());
    }
    let mut directories =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    // Finder-launched macOS apps do not inherit the interactive shell's PATH.
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    directories
        .into_iter()
        .map(|dir| dir.join(if cfg!(windows) { "yosys.exe" } else { "yosys" }))
        .find(|path| path.is_file())
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compile(
    source: &str,
    top: Option<&str>,
    frontend: Frontend,
    executable: PathBuf,
) -> Result<RtlModule, String> {
    let top = top.filter(|value| !value.is_empty());
    if top.is_some_and(|name| {
        !name.chars().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic() || c == '_' || (i > 0 && (c.is_ascii_digit() || c == '$'))
        })
    }) {
        return Err("Top module must be a simple Verilog identifier".into());
    }
    let scratch =
        Scratch(std::env::temp_dir().join(format!("openchippy-rtl-{}", uuid::Uuid::new_v4())));
    fs::create_dir(&scratch.0).map_err(|e| format!("Cannot create compiler workspace: {e}"))?;
    fs::write(scratch.0.join("source.sv"), source).map_err(|e| e.to_string())?;
    let selection = top.map_or_else(|| "-auto-top".into(), |name| format!("-top {name}"));
    let read = if matches!(frontend, Frontend::Slang) {
        format!(
            "plugin -i slang\nread_slang {} source.sv",
            top.map_or_else(String::new, |name| format!("--top {name}"))
        )
    } else {
        "read_verilog -sv -noautowire source.sv".into()
    };
    let script = format!("{read}\nhierarchy -check {selection}\nsynth -flatten -noabc {selection}\ndffunmap\ncheck -assert\nwrite_json netlist.json\n");
    fs::write(scratch.0.join("compile.ys"), script).map_err(|e| e.to_string())?;
    let log = fs::File::create(scratch.0.join("compiler.log")).map_err(|e| e.to_string())?;
    let mut child = Command::new(&executable)
        .args(["-Q", "-T", "-s", "compile.ys"])
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| format!("Cannot launch {}: {e}", executable.display()))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_secs(120) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Verilog compilation exceeded 120 seconds; reduce the elaborated design size"
                        .into(),
                );
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Cannot wait for compiler: {e}"));
            }
        }
    };
    if !status.success() {
        let log = fs::read_to_string(scratch.0.join("compiler.log")).unwrap_or_default();
        let tail = log
            .lines()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("Verilog compilation failed ({status}):\n{tail}"));
    }
    let path = scratch.0.join("netlist.json");
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Elaborated netlist exceeds the 64 MiB import limit".into());
    }
    let json =
        fs::read_to_string(path).map_err(|e| format!("Compiler did not produce a netlist: {e}"))?;
    let mut module = lower_json(&json, top)?;
    module.source = Some(source.into());
    module.compiler = Some(
        if matches!(frontend, Frontend::Slang) {
            "Yosys + slang"
        } else {
            "Yosys"
        }
        .into(),
    );
    Ok(module)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(untagged)]
enum Bit {
    Wire(u64),
    Constant(String),
}
#[derive(Deserialize)]
struct Netlist {
    modules: BTreeMap<String, Module>,
}
#[derive(Deserialize)]
struct Module {
    #[serde(default)]
    attributes: BTreeMap<String, serde_json::Value>,
    ports: BTreeMap<String, Port>,
    cells: BTreeMap<String, Cell>,
    #[serde(default)]
    netnames: BTreeMap<String, Net>,
}
#[derive(Deserialize)]
struct Port {
    direction: String,
    bits: Vec<Bit>,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    upto: usize,
}
#[derive(Deserialize)]
struct Net {
    bits: Vec<Bit>,
    #[serde(default)]
    attributes: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
struct Cell {
    #[serde(rename = "type")]
    kind: String,
    connections: BTreeMap<String, Vec<Bit>>,
}

fn lower_json(json: &str, top: Option<&str>) -> Result<RtlModule, String> {
    let netlist: Netlist =
        serde_json::from_str(json).map_err(|e| format!("Invalid compiler netlist: {e}"))?;
    let (name, module) = if let Some(top) = top {
        netlist
            .modules
            .get_key_value(top)
            .ok_or_else(|| format!("Compiler netlist has no top module {top}"))?
    } else {
        let candidates = netlist
            .modules
            .iter()
            .filter(|(_, m)| {
                m.attributes.get("top").is_some_and(|v| {
                    v.as_str().is_some_and(|s| s.ends_with('1')) || v.as_u64() == Some(1)
                })
            })
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            candidates[0]
        } else if netlist.modules.len() == 1 {
            netlist.modules.first_key_value().unwrap()
        } else {
            return Err("Select a top module for this compilation unit".into());
        }
    };
    if module.cells.len() > 100_000 {
        return Err("Elaborated design exceeds 100,000 cells".into());
    }
    let mut result = RtlModule {
        name: name.clone(),
        source: None,
        compiler: None,
        initial_values: BTreeMap::new(),
        parameters: vec![],
        ports: vec![],
        nets: vec![],
        instances: vec![],
        assignments: vec![],
        sequential_processes: vec![],
    };
    let mut wires = BTreeSet::new();
    for bit in module.ports.values().flat_map(|p| &p.bits).chain(
        module
            .cells
            .values()
            .flat_map(|c| c.connections.values().flatten()),
    ) {
        if let Bit::Wire(id) = bit {
            wires.insert(*id);
        }
    }
    // Internal bit names never inherit escaped hierarchy identifiers or collide
    // with top-level ports. The latter retain their declared range/direction.
    let mut prefix = "__oc_bit_".to_string();
    while module.ports.keys().any(|name| name.starts_with(&prefix)) {
        prefix.push('_');
    }
    let bit_name = |bit: &Bit| -> Result<String, String> {
        match bit {
            Bit::Wire(id) => Ok(format!("{prefix}{id}")),
            Bit::Constant(value) if matches!(value.as_str(), "0" | "1" | "x" | "z") => {
                Ok(format!("1'b{value}"))
            }
            _ => Err("Invalid constant in compiler netlist".into()),
        }
    };
    result.nets = wires
        .iter()
        .map(|id| RtlNet {
            name: format!("{prefix}{id}"),
            range: None,
        })
        .collect();
    let mut input_drivers = HashMap::new();
    for (name, port) in &module.ports {
        let direction = match port.direction.as_str() {
            "input" => RtlPortDirection::Input,
            "output" => RtlPortDirection::Output,
            _ => {
                return Err(format!(
                    "Bidirectional port {name} requires a resolved bidirectional simulator"
                ))
            }
        };
        if port.bits.is_empty() || port.bits.len() > 65536 {
            return Err(format!("Invalid or oversized port {name}"));
        }
        let high = port
            .offset
            .checked_add((port.bits.len() - 1) as i64)
            .ok_or("Port range overflow")?;
        let range = if port.bits.len() == 1 && port.offset == 0 && port.upto == 0 {
            None
        } else {
            Some(RtlRange {
                msb: if port.upto == 0 { high } else { port.offset },
                lsb: if port.upto == 0 { port.offset } else { high },
                msb_expression: None,
                lsb_expression: None,
            })
        };
        for (index, bit) in port.bits.iter().enumerate() {
            let external = if range.is_none() {
                name.clone()
            } else {
                format!(
                    "{name}[{}]",
                    if port.upto == 0 {
                        port.offset + index as i64
                    } else {
                        high - index as i64
                    }
                )
            };
            let internal = bit_name(bit)?;
            let (target, expression) = if direction == RtlPortDirection::Input {
                if !matches!(bit, Bit::Wire(_))
                    || input_drivers
                        .insert(bit.clone(), external.clone())
                        .is_some()
                {
                    return Err(format!(
                        "Aliased or constant input {external} cannot be simulated independently"
                    ));
                }
                (internal, external)
            } else {
                (external, internal)
            };
            result.assignments.push(RtlContinuousAssignment {
                target,
                referenced_signals: vec![expression.clone()],
                expression,
            });
        }
        result.ports.push(RtlPort {
            name: name.clone(),
            direction,
            range,
        });
    }
    for (name, cell) in &module.cells {
        // Yosys retains flattened source scopes as metadata, with no hardware pins.
        if cell.kind == "$scopeinfo" && cell.connections.is_empty() {
            continue;
        }
        let pin = |port: &str| -> Result<String, String> {
            let bits = cell
                .connections
                .get(port)
                .ok_or_else(|| format!("Cell {name} lacks pin {port}"))?;
            if bits.len() != 1 {
                return Err(format!("Cell {name}.{port} was not lowered to one bit"));
            }
            bit_name(&bits[0])
        };
        let primitive = match cell.kind.as_str() {
            "$_AND_" => Some((PrimitiveGate::And, "and", vec!["Y", "A", "B"])),
            "$_OR_" => Some((PrimitiveGate::Or, "or", vec!["Y", "A", "B"])),
            "$_XOR_" => Some((PrimitiveGate::Xor, "xor", vec!["Y", "A", "B"])),
            "$_XNOR_" => Some((PrimitiveGate::Xnor, "xnor", vec!["Y", "A", "B"])),
            "$_NAND_" => Some((PrimitiveGate::Nand, "nand", vec!["Y", "A", "B"])),
            "$_NOR_" => Some((PrimitiveGate::Nor, "nor", vec!["Y", "A", "B"])),
            "$_NOT_" => Some((PrimitiveGate::Not, "not", vec!["Y", "A"])),
            "$_BUF_" => Some((PrimitiveGate::Buf, "buf", vec!["Y", "A"])),
            _ => None,
        };
        if let Some((primitive, kind, pins)) = primitive {
            result.instances.push(RtlInstance {
                name: name.clone(),
                cell: kind.into(),
                primitive: Some(primitive),
                parameter_overrides: vec![],
                connections: pins.into_iter().map(pin).collect::<Result<_, _>>()?,
            });
        } else if matches!(cell.kind.as_str(), "$_MUX_" | "$_NMUX_") {
            let (a, b, s) = (pin("A")?, pin("B")?, pin("S")?);
            let expression = format!("({s} ? {b} : {a})");
            result.assignments.push(RtlContinuousAssignment {
                target: pin("Y")?,
                expression: if cell.kind == "$_NMUX_" {
                    format!("~{expression}")
                } else {
                    expression
                },
                referenced_signals: vec![a, b, s],
            });
        } else if let Some(kind) = cell
            .kind
            .strip_prefix("$_DFF_")
            .and_then(|s| s.strip_suffix('_'))
        {
            let chars = kind.chars().collect::<Vec<_>>();
            if !matches!(
                chars.as_slice(),
                ['P' | 'N'] | ['P' | 'N', 'P' | 'N', '0' | '1']
            ) {
                return Err(format!("Unsupported register cell {}", cell.kind));
            }
            let reset = if chars.len() == 3 {
                Some(RtlAsyncReset {
                    signal: pin("R")?,
                    active_high: chars[1] == 'P',
                    value: chars[2] == '1',
                })
            } else {
                None
            };
            let data = pin("D")?;
            result.sequential_processes.push(RtlSequentialProcess {
                edge: if chars[0] == 'P' {
                    RtlEdge::Posedge
                } else {
                    RtlEdge::Negedge
                },
                clock: pin("C")?,
                target: pin("Q")?,
                expression: data.clone(),
                referenced_signals: vec![data],
                asynchronous_reset: reset,
            });
        } else {
            return Err(format!("Cell {name} ({}) has no OpenChippy simulation model; import stopped rather than dropping hardware", cell.kind));
        }
    }
    for net in module.netnames.values() {
        if let Some(init) = net.attributes.get("init").and_then(|v| v.as_str()) {
            for (bit, value) in net.bits.iter().zip(init.chars().rev()) {
                if matches!(value, '0' | '1') && matches!(bit, Bit::Wire(_)) {
                    result.initial_values.insert(bit_name(bit)?, value == '1');
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{rtl_truth_table, rtl_waveform, LogicState, WaveformConfig};

    fn compiler(source: &str, top: &str, frontend: Frontend) -> RtlModule {
        compile(
            source,
            Some(top),
            frontend,
            find_yosys().expect("integration test requires OPENCHIPPY_YOSYS or yosys on PATH"),
        )
        .unwrap()
    }

    #[test]
    fn lower_aliases_constants_and_ascending_ports() {
        let source = r#"{"modules":{"top":{"ports":{"a":{"direction":"input","bits":[2,3],"offset":4,"upto":1},"y":{"direction":"output","bits":[3,2,"1"]}},"cells":{},"netnames":{}}}}"#;
        let module = lower_json(source, None).unwrap();
        assert_eq!(
            module.ports[0].range.as_ref().map(|r| (r.msb, r.lsb)),
            Some((4, 5))
        );
        let table = rtl_truth_table(&module).unwrap();
        for row in table.rows {
            assert_eq!(
                row.outputs,
                vec![LogicState::High, row.inputs[1], row.inputs[0]]
            );
        }
    }

    #[test]
    fn rejects_unmodeled_cells_and_inout_ports() {
        for source in [
            r#"{"modules":{"top":{"ports":{},"cells":{"latch":{"type":"$_DLATCH_P_","connections":{}}}}}}"#,
            r#"{"modules":{"top":{"ports":{"bus":{"direction":"inout","bits":[2]}},"cells":{}}}}"#,
        ] {
            assert!(lower_json(source, None).is_err());
        }
    }

    #[test]
    fn compiled_negative_port_indices_preserve_bit_order() {
        let source = r#"{"modules":{"top":{"ports":{"a":{"direction":"input","bits":[2,3],"offset":-2},"y":{"direction":"output","bits":[2,3],"offset":-2}},"cells":{}}}}"#;
        let module = lower_json(source, None).unwrap();
        let table = rtl_truth_table(&module).unwrap();
        assert_eq!(table.input_names, ["a[-1]", "a[-2]"]);
        for row in table.rows {
            assert_eq!(row.inputs, row.outputs);
        }
    }

    #[test]
    #[ignore = "requires a Yosys installation"]
    fn compiler_hierarchy_generate_functions_signed_arithmetic_and_source_roundtrip() {
        let source = r#"
`define WIDTH 3
module leaf #(parameter W=2)(input signed [W-1:0] a, input [W-1:0] b, output [W-1:0] y);
  function [W-1:0] add; input [W-1:0] x,z; begin add=x+z; end endfunction
  assign y = add(a >>> 1,b);
endmodule
module top(input signed [`WIDTH-1:0] a, input [`WIDTH-1:0] b, output [`WIDTH-1:0] y);
  wire [`WIDTH-1:0] n;
  leaf #(.W(`WIDTH)) u(.a(a),.b(b),.y(n));
  genvar i;
  generate for(i=0;i<`WIDTH;i=i+1) begin: bits assign y[i]=n[i]; end endgenerate
endmodule
"#;
        let module = compiler(source, "top", Frontend::Yosys);
        assert_eq!(export_structural_verilog(&module), source);
        let restored: RtlModule =
            serde_json::from_str(&serde_json::to_string(&module).unwrap()).unwrap();
        assert_eq!(module, restored);
        let table = rtl_truth_table(&module).unwrap();
        for row in table.rows {
            let value = |bits: &[LogicState]| {
                bits.iter()
                    .fold(0i32, |v, b| (v << 1) | i32::from(*b == LogicState::High))
            };
            assert!(row.converged);
            let a = value(&row.inputs[..3]);
            let a = if a >= 4 { a - 8 } else { a };
            assert_eq!(
                value(&row.outputs),
                ((a >> 1) + value(&row.inputs[3..])) & 7
            );
        }
    }

    #[test]
    #[ignore = "requires a Yosys installation"]
    fn compiler_async_reset_enable_and_nonblocking_registers() {
        let source = "module top(input clk, input reset, input enable, output reg [2:0] q); always @(posedge clk or posedge reset) if(reset) q<=0; else if(enable) q<=q+1'b1; endmodule";
        let module = compiler(source, "top", Frontend::Yosys);
        assert_eq!(module.sequential_processes.len(), 3);
        assert!(module
            .sequential_processes
            .iter()
            .all(|p| p.asynchronous_reset.is_some()));
        let wave = rtl_waveform(
            &module,
            WaveformConfig {
                duration_ns: 80,
                clock_period_ns: 10,
                input_change_ns: 10,
            },
        )
        .unwrap();
        let state = |name: &str, t: f64| {
            wave.signals
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .samples
                .iter()
                .rev()
                .find(|s| s.time_ns <= t)
                .unwrap()
                .state
        };
        let mut expected: Option<u8> = None;
        for t in (0..=80).step_by(5) {
            let reset = state("reset", t as f64) == LogicState::High;
            let enable = state("enable", t as f64) == LogicState::High;
            if reset {
                expected = Some(0);
            } else if t % 10 == 5 && enable {
                expected = expected.map(|v| (v + 1) & 7);
            }
            for bit in 0..3 {
                assert_eq!(
                    state(&format!("q[{bit}]"), t as f64),
                    expected.map_or(LogicState::Unknown, |v| if v & (1 << bit) != 0 {
                        LogicState::High
                    } else {
                        LogicState::Low
                    }),
                    "at {t}, bit {bit}"
                );
            }
        }
    }

    #[test]
    #[ignore = "requires a Yosys installation"]
    fn compiler_falling_clock_active_low_reset_and_pipeline() {
        let source = "module top(input tick, input reset_n, input d, output reg a, b); always @(negedge tick or negedge reset_n) if(!reset_n) begin a<=1'b1; b<=1'b0; end else begin a<=d; b<=a; end endmodule";
        let module = compiler(source, "top", Frontend::Yosys);
        let wave = rtl_waveform(&module, WaveformConfig {
            duration_ns: 80, clock_period_ns: 10, input_change_ns: 12,
        }).unwrap();
        let state = |name: &str, time: u32| {
            wave.signals.iter().find(|s| s.name == name).unwrap().samples.iter().rev()
                .find(|s| s.time_ns <= f64::from(time)).unwrap().state
        };
        let (mut a, mut b) = (LogicState::High, LogicState::Low);
        for time in 0..=80 {
            if state("reset_n", time) == LogicState::Low {
                a = LogicState::High;
                b = LogicState::Low;
            } else if time > 0 && time % 10 == 0 {
                b = a;
                a = state("d", time);
            }
            assert_eq!(state("a", time), a, "a at {time}");
            assert_eq!(state("b", time), b, "b at {time}");
        }
    }

    #[test]
    #[ignore = "requires a Yosys installation"]
    fn compiler_memory_read_write_and_initial_state() {
        let source="module top(input clk, input we, input [1:0] addr, input d, output q); reg mem[0:3]; integer i; initial for(i=0;i<4;i=i+1) mem[i]=0; always @(posedge clk) if(we) mem[addr]<=d; assign q=mem[addr]; endmodule";
        let module = compiler(source, "top", Frontend::Yosys);
        assert_eq!(module.initial_values.len(), 4);
        let wave = rtl_waveform(
            &module,
            WaveformConfig {
                duration_ns: 160,
                clock_period_ns: 10,
                input_change_ns: 5,
            },
        )
        .unwrap();
        let state = |name: &str, t: f64| {
            wave.signals
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .samples
                .iter()
                .rev()
                .find(|s| s.time_ns <= t)
                .unwrap()
                .state
        };
        let mut memory = [false; 4];
        for t in (0..=160).step_by(5) {
            let high = |name| state(name, t as f64) == LogicState::High;
            let addr = usize::from(high("addr[0]")) + 2 * usize::from(high("addr[1]"));
            if t % 10 == 5 && high("we") {
                memory[addr] = high("d");
            }
            assert_eq!(
                state("q", t as f64),
                if memory[addr] {
                    LogicState::High
                } else {
                    LogicState::Low
                },
                "at {t}"
            );
        }
    }

    #[test]
    #[ignore = "requires Yosys with the slang plugin"]
    fn compiler_slang_packages_structs_and_always_comb() {
        let source="package types; typedef struct packed {logic a; logic b;} pair; endpackage module top(input types::pair p, output logic y); always_comb y=p.a ^ p.b; endmodule";
        let module = compiler(source, "top", Frontend::Slang);
        let table = rtl_truth_table(&module).unwrap();
        assert_eq!(
            table.rows.iter().map(|r| r.outputs[0]).collect::<Vec<_>>(),
            vec![
                LogicState::Low,
                LogicState::High,
                LogicState::High,
                LogicState::Low
            ]
        );
    }
}

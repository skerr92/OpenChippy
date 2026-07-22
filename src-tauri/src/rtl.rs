use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtlPortDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlRange {
    pub msb: usize,
    pub lsb: usize,
    #[serde(default)]
    pub msb_expression: Option<String>,
    #[serde(default)]
    pub lsb_expression: Option<String>,
}

impl RtlRange {
    pub fn width(&self) -> usize {
        self.msb.abs_diff(self.lsb) + 1
    }

    fn contains(&self, index: usize) -> bool {
        index >= self.msb.min(self.lsb) && index <= self.msb.max(self.lsb)
    }

    fn indices(&self) -> Box<dyn Iterator<Item = usize>> {
        if self.msb >= self.lsb {
            Box::new((self.lsb..=self.msb).rev())
        } else {
            Box::new(self.msb..=self.lsb)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlPort {
    pub name: String,
    pub direction: RtlPortDirection,
    #[serde(default)]
    pub range: Option<RtlRange>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlNet {
    pub name: String,
    #[serde(default)]
    pub range: Option<RtlRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveGate {
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
    Not,
    Buf,
}

impl PrimitiveGate {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "and" => Self::And,
            "or" => Self::Or,
            "xor" => Self::Xor,
            "nand" => Self::Nand,
            "nor" => Self::Nor,
            "xnor" => Self::Xnor,
            "not" => Self::Not,
            "buf" => Self::Buf,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlInstance {
    pub name: String,
    pub cell: String,
    #[serde(default)]
    pub primitive: Option<PrimitiveGate>,
    #[serde(default)]
    pub parameter_overrides: Vec<RtlParameterOverride>,
    /// Verilog primitive order: output first, followed by inputs.
    pub connections: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlParameter {
    pub name: String,
    pub default_expression: String,
    pub default_value: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlParameterOverride {
    pub name: Option<String>,
    pub expression: String,
    pub value: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlModule {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<RtlParameter>,
    pub ports: Vec<RtlPort>,
    pub nets: Vec<RtlNet>,
    pub instances: Vec<RtlInstance>,
    #[serde(default)]
    pub assignments: Vec<RtlContinuousAssignment>,
    #[serde(default)]
    pub sequential_processes: Vec<RtlSequentialProcess>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlContinuousAssignment {
    pub target: String,
    pub expression: String,
    pub referenced_signals: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtlEdge {
    Posedge,
    Negedge,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlSequentialProcess {
    pub edge: RtlEdge,
    pub clock: String,
    pub target: String,
    pub expression: String,
    pub referenced_signals: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlGatePlacement {
    pub instance_name: String,
    pub x: f64,
    pub y: f64,
    pub level: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtlDesign {
    pub module: RtlModule,
    pub placements: Vec<RtlGatePlacement>,
}

pub fn map_module(module: RtlModule) -> RtlDesign {
    let output_driver = module
        .instances
        .iter()
        .enumerate()
        .filter_map(|(index, instance)| {
            instance.connections.first().map(|net| (net.clone(), index))
        })
        .collect::<HashMap<_, _>>();
    let mut levels = vec![None; module.instances.len()];
    let mut unresolved = (0..module.instances.len()).collect::<Vec<_>>();
    while !unresolved.is_empty() {
        let before = unresolved.len();
        unresolved.retain(|index| {
            let dependencies = module.instances[*index]
                .connections
                .iter()
                .skip(1)
                .filter_map(|net| output_driver.get(net).copied())
                .collect::<Vec<_>>();
            if dependencies
                .iter()
                .all(|dependency| levels[*dependency].is_some())
            {
                levels[*index] = Some(
                    dependencies
                        .iter()
                        .filter_map(|dependency| levels[*dependency])
                        .max()
                        .map_or(0, |level| level + 1),
                );
                false
            } else {
                true
            }
        });
        if unresolved.len() == before {
            let fallback = levels
                .iter()
                .flatten()
                .copied()
                .max()
                .map_or(0, |level| level + 1);
            for index in unresolved.drain(..) {
                levels[index] = Some(fallback);
            }
        }
    }
    let mut rows = HashMap::<usize, usize>::new();
    let placements = module
        .instances
        .iter()
        .enumerate()
        .map(|(index, instance)| {
            let level = levels[index].unwrap_or_default();
            let row = rows.entry(level).or_default();
            let placement = RtlGatePlacement {
                instance_name: instance.name.clone(),
                x: 8.0 + level as f64 * 8.0,
                y: 6.0 + *row as f64 * 5.0,
                level,
            };
            *row += 1;
            placement
        })
        .collect();
    RtlDesign { module, placements }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Ident(String),
    Number(usize),
    String(String),
    Symbol(char),
}

fn is_binary_literal_tail(value: &str) -> bool {
    value
        .strip_prefix('b')
        .is_some_and(|bits| !bits.is_empty() && bits.chars().all(|bit| matches!(bit, '0' | '1')))
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_whitespace() {
            continue;
        }
        if character == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    break;
                }
            }
            continue;
        }
        if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut closed = false;
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("unterminated block comment".into());
            }
            continue;
        }
        if character == '('
            && chars.peek() == Some(&'*')
            && tokens.last() != Some(&Token::Symbol('@'))
        {
            chars.next();
            let mut closed = false;
            let mut quoted = false;
            let mut escaped = false;
            while let Some(next) = chars.next() {
                if quoted {
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if next == '\\' {
                        escaped = true;
                        continue;
                    }
                    if next == '"' {
                        quoted = false;
                    }
                } else if next == '"' {
                    quoted = true;
                } else if next == '*' && chars.peek() == Some(&')') {
                    chars.next();
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("unterminated Verilog attribute".into());
            }
            continue;
        }
        if character == '"' {
            let mut value = String::new();
            let mut escaped = false;
            let mut closed = false;
            for next in chars.by_ref() {
                if escaped {
                    value.push(next);
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    value.push(next);
                    escaped = true;
                    continue;
                }
                if next == '"' {
                    closed = true;
                    break;
                }
                value.push(next);
            }
            if !closed {
                return Err("unterminated Verilog string literal".into());
            }
            tokens.push(Token::String(value));
            continue;
        }
        if character.is_ascii_alphabetic() || character == '_' {
            let mut identifier = String::from(character);
            while chars
                .peek()
                .is_some_and(|next| next.is_ascii_alphanumeric() || *next == '_' || *next == '$')
            {
                identifier.push(chars.next().unwrap());
            }
            tokens.push(Token::Ident(identifier));
            continue;
        }
        if character.is_ascii_digit() {
            let mut number = String::from(character);
            while chars.peek().is_some_and(|next| next.is_ascii_digit()) {
                number.push(chars.next().unwrap());
            }
            tokens.push(Token::Number(number.parse().unwrap()));
            continue;
        }
        if "(),;=~[]:'#.+-*/&|^@?<{}".contains(character) {
            tokens.push(Token::Symbol(character));
            continue;
        }
        return Err(format!("unsupported Verilog character: {character}"));
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self, String> {
        Ok(Self {
            tokens: tokenize(source)?,
            cursor: 0,
        })
    }

    fn ident(&mut self) -> Result<String, String> {
        match self.tokens.get(self.cursor).cloned() {
            Some(Token::Ident(value)) => {
                self.cursor += 1;
                Ok(value)
            }
            other => Err(format!("expected identifier, found {other:?}")),
        }
    }

    fn symbol(&mut self, expected: char) -> Result<(), String> {
        match self.tokens.get(self.cursor) {
            Some(Token::Symbol(actual)) if *actual == expected => {
                self.cursor += 1;
                Ok(())
            }
            other => Err(format!("expected '{expected}', found {other:?}")),
        }
    }

    fn number(&mut self) -> Result<usize, String> {
        match self.tokens.get(self.cursor).cloned() {
            Some(Token::Number(value)) => {
                self.cursor += 1;
                Ok(value)
            }
            other => Err(format!("expected non-negative integer, found {other:?}")),
        }
    }

    fn optional_range(
        &mut self,
        parameters: &HashMap<String, i64>,
    ) -> Result<Option<RtlRange>, String> {
        if !self.peek_symbol('[') {
            return Ok(None);
        }
        self.symbol('[')?;
        let msb_start = self.cursor;
        let msb = self.constant_expression(parameters)?;
        let msb_expression = self.tokens_text(msb_start, self.cursor);
        self.symbol(':')?;
        let lsb_start = self.cursor;
        let lsb = self.constant_expression(parameters)?;
        let lsb_expression = self.tokens_text(lsb_start, self.cursor);
        self.symbol(']')?;
        if msb < 0 || lsb < 0 {
            return Err("packed range bounds must elaborate to non-negative integers".into());
        }
        Ok(Some(RtlRange {
            msb: msb as usize,
            lsb: lsb as usize,
            msb_expression: Some(msb_expression),
            lsb_expression: Some(lsb_expression),
        }))
    }

    fn constant_expression(&mut self, parameters: &HashMap<String, i64>) -> Result<i64, String> {
        self.additive_expression(parameters)
    }

    fn additive_expression(&mut self, parameters: &HashMap<String, i64>) -> Result<i64, String> {
        let mut value = self.multiplicative_expression(parameters)?;
        loop {
            if self.peek_symbol('+') {
                self.cursor += 1;
                value = value
                    .checked_add(self.multiplicative_expression(parameters)?)
                    .ok_or_else(|| "parameter expression overflowed".to_string())?;
            } else if self.peek_symbol('-') {
                self.cursor += 1;
                value = value
                    .checked_sub(self.multiplicative_expression(parameters)?)
                    .ok_or_else(|| "parameter expression overflowed".to_string())?;
            } else {
                return Ok(value);
            }
        }
    }

    fn multiplicative_expression(
        &mut self,
        parameters: &HashMap<String, i64>,
    ) -> Result<i64, String> {
        let mut value = self.primary_expression(parameters)?;
        loop {
            if self.peek_symbol('*') {
                self.cursor += 1;
                value = value
                    .checked_mul(self.primary_expression(parameters)?)
                    .ok_or_else(|| "parameter expression overflowed".to_string())?;
            } else if self.peek_symbol('/') {
                self.cursor += 1;
                let divisor = self.primary_expression(parameters)?;
                if divisor == 0 {
                    return Err("parameter expression divides by zero".into());
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }

    fn primary_expression(&mut self, parameters: &HashMap<String, i64>) -> Result<i64, String> {
        if self.peek_symbol('(') {
            self.cursor += 1;
            let value = self.constant_expression(parameters)?;
            self.symbol(')')?;
            return Ok(value);
        }
        if self.peek_symbol('-') {
            self.cursor += 1;
            return self
                .primary_expression(parameters)?
                .checked_neg()
                .ok_or_else(|| "parameter expression overflowed".into());
        }
        match self.tokens.get(self.cursor).cloned() {
            Some(Token::Number(value)) => {
                self.cursor += 1;
                Ok(value as i64)
            }
            Some(Token::Ident(name)) => {
                self.cursor += 1;
                parameters
                    .get(&name)
                    .copied()
                    .ok_or_else(|| format!("unknown parameter in constant expression: {name}"))
            }
            other => Err(format!("expected constant expression, found {other:?}")),
        }
    }

    fn signal_ref(&mut self) -> Result<String, String> {
        if self.peek_symbol('.') {
            return Err(
                "named port connections are not supported yet; use positional connections".into(),
            );
        }
        if matches!(self.tokens.get(self.cursor), Some(Token::Number(_))) {
            let value = self.number()?;
            if self.peek_symbol('\'') {
                self.symbol('\'')?;
                let base_value = self.ident()?;
                if value != 1 || !matches!(base_value.as_str(), "b0" | "b1") {
                    return Err("only one-bit binary constants 1'b0 and 1'b1 are supported in structural connections".into());
                }
                return Ok(format!("1'{base_value}"));
            }
            return match value {
                0 | 1 => Ok(format!("1'b{value}")),
                _ => Err("only scalar constants 0, 1, 1'b0, and 1'b1 are supported in structural connections".into()),
            };
        }
        let name = self.ident()?;
        if !self.peek_symbol('[') {
            return Ok(name);
        }
        self.symbol('[')?;
        let index = self.number()?;
        if self.peek_symbol(':') {
            return Err(
                "part selects are not supported yet; use a constant single-bit select".into(),
            );
        }
        self.symbol(']')?;
        Ok(format!("{name}[{index}]"))
    }

    fn comma_signal_refs(&mut self) -> Result<Vec<String>, String> {
        let mut signals = vec![self.signal_ref()?];
        while self.peek_symbol(',') {
            self.cursor += 1;
            signals.push(self.signal_ref()?);
        }
        Ok(signals)
    }

    fn peek_ident(&self, expected: &str) -> bool {
        matches!(self.tokens.get(self.cursor), Some(Token::Ident(value)) if value == expected)
    }

    fn peek_symbol(&self, expected: char) -> bool {
        self.tokens.get(self.cursor) == Some(&Token::Symbol(expected))
    }

    fn comma_names(&mut self) -> Result<Vec<String>, String> {
        let mut names = vec![self.ident()?];
        while self.peek_symbol(',') {
            self.cursor += 1;
            names.push(self.ident()?);
        }
        Ok(names)
    }

    fn tokens_text(&self, start: usize, end: usize) -> String {
        self.tokens[start..end]
            .iter()
            .map(|token| match token {
                Token::Ident(value) => value.clone(),
                Token::Number(value) => value.to_string(),
                Token::String(value) => format!("\"{value}\""),
                Token::Symbol(value) => value.to_string(),
            })
            .collect()
    }

    fn blocking_assignment(
        &mut self,
        parameters: &HashMap<String, i64>,
    ) -> Result<RtlContinuousAssignment, String> {
        let target = self.signal_ref()?;
        if self.peek_symbol('<') {
            return Err("nonblocking assignment requires an edge-triggered always_ff/always process; use blocking '=' in combinational logic".into());
        }
        self.symbol('=')?;
        let expression_start = self.cursor;
        while !self.peek_symbol(';') {
            if self.cursor >= self.tokens.len() {
                return Err("procedural blocking assignment is missing ';'".into());
            }
            self.cursor += 1;
        }
        if self.cursor == expression_start {
            return Err("procedural blocking assignment expression cannot be empty".into());
        }
        let expression = self.tokens_text(expression_start, self.cursor);
        let mut referenced_signals = Vec::new();
        for token in &self.tokens[expression_start..self.cursor] {
            if let Token::Ident(value) = token {
                if !is_binary_literal_tail(value)
                    && !parameters.contains_key(value)
                    && !referenced_signals.contains(value)
                {
                    referenced_signals.push(value.clone());
                }
            }
        }
        self.symbol(';')?;
        Ok(RtlContinuousAssignment {
            target,
            expression,
            referenced_signals,
        })
    }

    fn conditional_assignment(
        &mut self,
        parameters: &HashMap<String, i64>,
    ) -> Result<RtlContinuousAssignment, String> {
        if self.ident()? != "if" {
            return Err("expected if statement".into());
        }
        self.symbol('(')?;
        let condition_start = self.cursor;
        let mut depth = 1usize;
        while depth > 0 {
            match self.tokens.get(self.cursor) {
                Some(Token::Symbol('(')) => depth += 1,
                Some(Token::Symbol(')')) => depth -= 1,
                Some(_) => {}
                None => return Err("if condition is missing ')'".into()),
            }
            if depth > 0 {
                self.cursor += 1;
            }
        }
        if self.cursor == condition_start {
            return Err("if condition cannot be empty".into());
        }
        let condition_end = self.cursor;
        let condition = self.tokens_text(condition_start, condition_end);
        self.symbol(')')?;

        let branch = |parser: &mut Parser| -> Result<RtlContinuousAssignment, String> {
            let block = parser.peek_ident("begin");
            if block {
                parser.cursor += 1;
            }
            if parser.peek_ident("if") {
                return Err(
                    "nested procedural conditionals are not supported in this slice".into(),
                );
            }
            let assignment = parser.blocking_assignment(parameters)?;
            if block {
                if !parser.peek_ident("end") {
                    return Err("each conditional branch currently requires exactly one blocking assignment".into());
                }
                parser.cursor += 1;
            }
            Ok(assignment)
        };
        let when_true = branch(self)?;
        if !self.peek_ident("else") {
            return Err(format!(
                "if statement assigning {} needs an else branch to avoid an inferred latch",
                when_true.target
            ));
        }
        self.cursor += 1;
        let when_false = branch(self)?;
        if when_true.target != when_false.target {
            return Err(
                "if and else branches must definitely assign the same target in this slice".into(),
            );
        }
        let mut referenced_signals = Vec::new();
        for token in &self.tokens[condition_start..condition_end] {
            if let Token::Ident(value) = token {
                if !parameters.contains_key(value) && !referenced_signals.contains(value) {
                    referenced_signals.push(value.clone());
                }
            }
        }
        for reference in when_true
            .referenced_signals
            .iter()
            .chain(&when_false.referenced_signals)
        {
            if !referenced_signals.contains(reference) {
                referenced_signals.push(reference.clone());
            }
        }
        Ok(RtlContinuousAssignment {
            target: when_true.target,
            expression: format!(
                "({condition})?({}):({})",
                when_true.expression, when_false.expression
            ),
            referenced_signals,
        })
    }

    fn case_assignment(
        &mut self,
        parameters: &HashMap<String, i64>,
    ) -> Result<RtlContinuousAssignment, String> {
        if self.ident()? != "case" {
            return Err("expected case statement".into());
        }
        self.symbol('(')?;
        let selector_start = self.cursor;
        while !self.peek_symbol(')') {
            if self.cursor >= self.tokens.len() {
                return Err("case selector is missing ')'".into());
            }
            self.cursor += 1;
        }
        if self.cursor == selector_start {
            return Err("case selector cannot be empty".into());
        }
        let selector_end = self.cursor;
        let selector = self.tokens_text(selector_start, selector_end);
        self.symbol(')')?;
        let mut arms = Vec::<(String, RtlContinuousAssignment)>::new();
        let mut fallback = None;
        while !self.peek_ident("endcase") {
            if self.cursor >= self.tokens.len() {
                return Err("case statement is missing endcase".into());
            }
            let is_default = self.peek_ident("default");
            let label = if is_default {
                self.cursor += 1;
                String::new()
            } else {
                let start = self.cursor;
                while !self.peek_symbol(':') {
                    if self.cursor >= self.tokens.len() {
                        return Err("case item is missing ':'".into());
                    }
                    self.cursor += 1;
                }
                if self.cursor == start {
                    return Err("case item value cannot be empty".into());
                }
                self.tokens_text(start, self.cursor)
            };
            self.symbol(':')?;
            let assignment = self.blocking_assignment(parameters)?;
            if is_default {
                if fallback.replace(assignment).is_some() {
                    return Err("case statement has more than one default item".into());
                }
            } else {
                arms.push((label, assignment));
            }
        }
        self.cursor += 1;
        let fallback = fallback.ok_or_else(|| {
            "case statement needs a default item to avoid an inferred latch".to_string()
        })?;
        if arms.is_empty() {
            return Err("case statement needs at least one value item".into());
        }
        if arms.iter().any(|(_, arm)| arm.target != fallback.target) {
            return Err(
                "all case items must definitely assign the same target in this slice".into(),
            );
        }
        let mut expression = format!("({})", fallback.expression);
        for (label, arm) in arms.iter().rev() {
            expression = format!(
                "(({selector})==({label}))?({}):({expression})",
                arm.expression
            );
        }
        let mut referenced_signals = Vec::new();
        for token in &self.tokens[selector_start..selector_end] {
            if let Token::Ident(value) = token {
                if !parameters.contains_key(value) && !referenced_signals.contains(value) {
                    referenced_signals.push(value.clone());
                }
            }
        }
        for reference in arms
            .iter()
            .flat_map(|(_, arm)| &arm.referenced_signals)
            .chain(&fallback.referenced_signals)
        {
            if !referenced_signals.contains(reference) {
                referenced_signals.push(reference.clone());
            }
        }
        Ok(RtlContinuousAssignment {
            target: fallback.target,
            expression,
            referenced_signals,
        })
    }

    fn sequential_process(
        &mut self,
        parameters: &HashMap<String, i64>,
        always_ff: bool,
    ) -> Result<RtlSequentialProcess, String> {
        self.symbol('@')?;
        self.symbol('(')?;
        let edge = match self.ident()?.as_str() {
            "posedge" => RtlEdge::Posedge,
            "negedge" => RtlEdge::Negedge,
            _ => return Err("clocked always blocks require posedge or negedge".into()),
        };
        let clock = self.signal_ref()?;
        self.symbol(')')?;
        let block = self.peek_ident("begin");
        if block {
            self.cursor += 1;
        }
        let target = self.signal_ref()?;
        self.symbol('<')?;
        self.symbol('=')?;
        let expression_start = self.cursor;
        while !self.peek_symbol(';') {
            if self.cursor >= self.tokens.len() {
                return Err("nonblocking assignment is missing ';'".into());
            }
            self.cursor += 1;
        }
        if self.cursor == expression_start {
            return Err("nonblocking assignment expression cannot be empty".into());
        }
        let expression = self.tokens_text(expression_start, self.cursor);
        let mut referenced_signals = Vec::new();
        for token in &self.tokens[expression_start..self.cursor] {
            if let Token::Ident(value) = token {
                if !is_binary_literal_tail(value)
                    && !parameters.contains_key(value)
                    && !referenced_signals.contains(value)
                {
                    referenced_signals.push(value.clone());
                }
            }
        }
        self.symbol(';')?;
        if block {
            if !self.peek_ident("end") {
                return Err(
                    "clocked process currently requires exactly one nonblocking assignment".into(),
                );
            }
            self.cursor += 1;
        }
        let _ = always_ff;
        Ok(RtlSequentialProcess {
            edge,
            clock,
            target,
            expression,
            referenced_signals,
        })
    }
}

pub fn parse_structural_verilog(source: &str) -> Result<RtlModule, String> {
    if source.split_whitespace().any(|token| token == "initial") {
        return Err("procedural initial blocks are not supported yet; imported hardware must have deterministic synthesizable startup semantics".into());
    }
    if source
        .split_whitespace()
        .filter(|token| *token == "module")
        .count()
        > 1
    {
        return Err("only one module is supported per import".into());
    }
    let mut parser = Parser::new(source)?;
    if parser.ident()? != "module" {
        return Err("source must start with one module".into());
    }
    let name = parser.ident()?;
    let mut parameters = Vec::new();
    let mut parameter_values = HashMap::new();
    if parser.peek_symbol('#') {
        parser.symbol('#')?;
        parser.symbol('(')?;
        while !parser.peek_symbol(')') {
            if parser.peek_ident("parameter") {
                parser.cursor += 1;
            }
            if parser.peek_ident("integer") {
                parser.cursor += 1;
            }
            let parameter_name = parser.ident()?;
            if parameter_values.contains_key(&parameter_name) {
                return Err(format!(
                    "parameter declared more than once: {parameter_name}"
                ));
            }
            parser.symbol('=')?;
            let expression_start = parser.cursor;
            let default_value = parser.constant_expression(&parameter_values).map_err(|error| {
                format!("parameter {parameter_name} could not be elaborated (forward references or cycles are not allowed): {error}")
            })?;
            let default_expression = parser.tokens_text(expression_start, parser.cursor);
            parameter_values.insert(parameter_name.clone(), default_value);
            parameters.push(RtlParameter {
                name: parameter_name,
                default_expression,
                default_value,
            });
            if parser.peek_symbol(',') {
                parser.cursor += 1;
            } else if !parser.peek_symbol(')') {
                return Err("expected ',' or ')' in parameter declaration list".into());
            }
        }
        parser.symbol(')')?;
    }
    parser.symbol('(')?;
    let mut header_names = Vec::new();
    let mut directions = HashMap::new();
    let mut ranges = HashMap::new();
    let mut current_direction = None;
    let mut current_range = None;
    while !parser.peek_symbol(')') {
        if parser.peek_ident("input") || parser.peek_ident("output") {
            current_direction = Some(if parser.ident()? == "input" {
                RtlPortDirection::Input
            } else {
                RtlPortDirection::Output
            });
            if parser.peek_ident("wire") || parser.peek_ident("logic") || parser.peek_ident("reg") {
                parser.cursor += 1;
            }
            if parser.peek_ident("signed") || parser.peek_ident("unsigned") {
                parser.cursor += 1;
            }
            current_range = parser.optional_range(&parameter_values)?;
        }
        let port = parser.ident()?;
        if header_names.contains(&port) {
            return Err(format!("port declared more than once: {port}"));
        }
        header_names.push(port.clone());
        if let Some(direction) = current_direction {
            directions.insert(port.clone(), direction);
            ranges.insert(port, current_range.clone());
        }
        if parser.peek_symbol(',') {
            parser.cursor += 1;
        } else if !parser.peek_symbol(')') {
            return Err("expected ',' or ')' in module header".into());
        }
    }
    parser.symbol(')')?;
    parser.symbol(';')?;

    let mut nets = Vec::new();
    let mut instances = Vec::new();
    let mut assignments = Vec::new();
    let mut sequential_processes = Vec::new();
    while !parser.peek_ident("endmodule") {
        if parser.cursor >= parser.tokens.len() {
            return Err("module is missing endmodule".into());
        }
        if parser.peek_ident("input") || parser.peek_ident("output") {
            let direction = if parser.ident()? == "input" {
                RtlPortDirection::Input
            } else {
                RtlPortDirection::Output
            };
            if parser.peek_ident("wire") || parser.peek_ident("logic") || parser.peek_ident("reg") {
                parser.cursor += 1;
            }
            if parser.peek_ident("signed") || parser.peek_ident("unsigned") {
                parser.cursor += 1;
            }
            let range = parser.optional_range(&parameter_values)?;
            for port in parser.comma_names()? {
                if !header_names.contains(&port) {
                    return Err(format!("declared port {port} is absent from module header"));
                }
                if directions.insert(port.clone(), direction).is_some() {
                    return Err(format!("port direction declared more than once: {port}"));
                }
                ranges.insert(port, range.clone());
            }
            parser.symbol(';')?;
            continue;
        }
        if parser.peek_ident("wire") {
            parser.cursor += 1;
            let range = parser.optional_range(&parameter_values)?;
            nets.extend(parser.comma_names()?.into_iter().map(|name| RtlNet {
                name,
                range: range.clone(),
            }));
            parser.symbol(';')?;
            continue;
        }
        if parser.peek_ident("assign") {
            parser.cursor += 1;
            let output = parser.signal_ref()?;
            parser.symbol('=')?;
            let expression_start = parser.cursor;
            let simple_start = parser.cursor;
            let inverted = if parser.peek_symbol('~') {
                parser.cursor += 1;
                true
            } else {
                false
            };
            let simple_input = parser.signal_ref();
            if let Ok(input) = simple_input {
                if parser.peek_symbol(';') {
                    parser.symbol(';')?;
                    instances.push(RtlInstance {
                        name: format!("assign${}", instances.len() + 1),
                        cell: if inverted { "not" } else { "buf" }.into(),
                        primitive: Some(if inverted {
                            PrimitiveGate::Not
                        } else {
                            PrimitiveGate::Buf
                        }),
                        parameter_overrides: vec![],
                        connections: vec![output, input],
                    });
                    continue;
                }
            }
            parser.cursor = simple_start;
            while !parser.peek_symbol(';') {
                if parser.cursor >= parser.tokens.len() {
                    return Err("continuous assignment is missing ';'".into());
                }
                parser.cursor += 1;
            }
            if parser.cursor == expression_start {
                return Err("continuous assignment expression cannot be empty".into());
            }
            let expression = parser.tokens_text(expression_start, parser.cursor);
            let mut referenced_signals = Vec::new();
            for token in &parser.tokens[expression_start..parser.cursor] {
                if let Token::Ident(value) = token {
                    if !is_binary_literal_tail(value)
                        && !parameter_values.contains_key(value)
                        && !referenced_signals.contains(value)
                    {
                        referenced_signals.push(value.clone());
                    }
                }
            }
            parser.symbol(';')?;
            assignments.push(RtlContinuousAssignment {
                target: output,
                expression,
                referenced_signals,
            });
            continue;
        }
        let classic_edge = parser.peek_ident("always")
            && matches!(
                parser.tokens.get(parser.cursor + 1),
                Some(Token::Symbol('@'))
            )
            && matches!(
                parser.tokens.get(parser.cursor + 2),
                Some(Token::Symbol('('))
            )
            && matches!(parser.tokens.get(parser.cursor + 3), Some(Token::Ident(edge)) if edge == "posedge" || edge == "negedge");
        if parser.peek_ident("always_ff") || classic_edge {
            let always_ff = parser.ident()? == "always_ff";
            sequential_processes.push(parser.sequential_process(&parameter_values, always_ff)?);
            continue;
        }
        if parser.peek_ident("always_comb") || parser.peek_ident("always") {
            let explicit_comb = parser.ident()? == "always_comb";
            if !explicit_comb {
                if !parser.peek_symbol('@') {
                    return Err("procedural always blocks require an explicit combinational @* sensitivity in this slice".into());
                }
                parser.symbol('@')?;
                if parser.peek_symbol('(') {
                    parser.symbol('(')?;
                    parser.symbol('*')?;
                    parser.symbol(')')?;
                } else {
                    parser.symbol('*')?;
                }
            }
            let block = parser.peek_ident("begin");
            if block {
                parser.cursor += 1;
            }
            let mut count = 0;
            while !block || !parser.peek_ident("end") {
                let assignment = if parser.peek_ident("case") {
                    parser.case_assignment(&parameter_values)?
                } else if parser.peek_ident("if") {
                    parser.conditional_assignment(&parameter_values)?
                } else {
                    parser.blocking_assignment(&parameter_values)?
                };
                assignments.push(assignment);
                count += 1;
                if !block {
                    break;
                }
            }
            if block {
                parser.cursor += 1;
            }
            if count == 0 {
                return Err("combinational always block cannot be empty".into());
            }
            continue;
        }
        let cell = parser.ident()?;
        let primitive = PrimitiveGate::parse(&cell);
        let mut parameter_overrides = Vec::new();
        if parser.peek_symbol('#') {
            parser.symbol('#')?;
            parser.symbol('(')?;
            let mut named = None;
            let mut names = HashSet::new();
            while !parser.peek_symbol(')') {
                let override_name = if parser.peek_symbol('.') {
                    parser.cursor += 1;
                    let name = parser.ident()?;
                    parser.symbol('(')?;
                    named.get_or_insert(true);
                    Some(name)
                } else {
                    named.get_or_insert(false);
                    None
                };
                if named != Some(override_name.is_some()) {
                    return Err("cannot mix named and positional parameter overrides".into());
                }
                if let Some(name) = &override_name {
                    if !names.insert(name.clone()) {
                        return Err(format!(
                            "parameter override specified more than once: {name}"
                        ));
                    }
                }
                let expression_start = parser.cursor;
                let value = parser.constant_expression(&parameter_values)?;
                let expression = parser.tokens_text(expression_start, parser.cursor);
                if override_name.is_some() {
                    parser.symbol(')')?;
                }
                parameter_overrides.push(RtlParameterOverride {
                    name: override_name,
                    expression,
                    value,
                });
                if parser.peek_symbol(',') {
                    parser.cursor += 1;
                } else if !parser.peek_symbol(')') {
                    return Err("expected ',' or ')' in parameter override list".into());
                }
            }
            parser.symbol(')')?;
        }
        let instance_name = parser.ident()?;
        parser.symbol('(')?;
        if parser.peek_symbol('.') {
            return Err(
                "named port connections are not supported yet; use positional connections".into(),
            );
        }
        let connections = parser.comma_signal_refs()?;
        parser.symbol(')')?;
        parser.symbol(';')?;
        if let Some(primitive) = primitive {
            let expected_minimum = if matches!(primitive, PrimitiveGate::Not | PrimitiveGate::Buf) {
                2
            } else {
                3
            };
            if connections.len() < expected_minimum {
                return Err(format!("{instance_name} has too few connections"));
            }
            if !parameter_overrides.is_empty() {
                return Err(format!(
                    "built-in primitive {cell} does not accept parameter overrides"
                ));
            }
        }
        instances.push(RtlInstance {
            name: instance_name,
            cell,
            primitive,
            parameter_overrides,
            connections,
        });
    }
    parser.cursor += 1;
    if parser.cursor != parser.tokens.len() {
        return Err("only one module is supported per import".into());
    }

    let ports = header_names
        .into_iter()
        .map(|name| {
            let direction = directions
                .get(&name)
                .copied()
                .ok_or_else(|| format!("port {name} has no input/output direction"))?;
            Ok(RtlPort {
                range: ranges.get(&name).cloned().flatten(),
                name,
                direction,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut declarations = HashSet::new();
    let mut declared_ranges = HashMap::new();
    let mut known = HashSet::new();
    for port in &ports {
        declarations.insert(port.name.clone());
        declared_ranges.insert(port.name.clone(), port.range.clone());
        if let Some(range) = &port.range {
            known.extend(
                range
                    .indices()
                    .map(|index| format!("{}[{index}]", port.name)),
            );
        } else {
            known.insert(port.name.clone());
        }
    }
    for net in &nets {
        if !declarations.insert(net.name.clone()) {
            return Err(format!("signal declared more than once: {}", net.name));
        }
        declared_ranges.insert(net.name.clone(), net.range.clone());
        if let Some(range) = &net.range {
            known.extend(
                range
                    .indices()
                    .map(|index| format!("{}[{index}]", net.name)),
            );
        } else {
            known.insert(net.name.clone());
        }
    }
    let instance_names = instances
        .iter()
        .map(|instance| instance.name.as_str())
        .collect::<HashSet<_>>();
    if instance_names.len() != instances.len() {
        return Err("instance names must be unique".into());
    }
    for instance in &instances {
        for connection in &instance.connections {
            if matches!(connection.as_str(), "1'b0" | "1'b1") {
                continue;
            }
            if let Some(Some(range)) = declared_ranges.get(connection) {
                if instance.primitive.is_some() {
                    return Err(format!(
                        "connection {connection} is {} bits wide; primitive pins require a constant bit select",
                        range.width()
                    ));
                }
                continue;
            }
            if !known.contains(connection.as_str()) {
                if let Some((base, index)) = parse_bit_name(connection) {
                    if let Some(Some(range)) = declared_ranges.get(base) {
                        if !range.contains(index) {
                            return Err(format!(
                                "bit select {connection} is outside declared range [{}:{}]",
                                range.msb, range.lsb
                            ));
                        }
                    }
                }
                return Err(format!(
                    "connection references undeclared signal: {connection}"
                ));
            }
        }
    }
    for assignment in &assignments {
        if !declarations.contains(&assignment.target) && !known.contains(&assignment.target) {
            return Err(format!(
                "continuous assignment target is undeclared: {}",
                assignment.target
            ));
        }
        for reference in &assignment.referenced_signals {
            if !declarations.contains(reference) && !known.contains(reference) {
                return Err(format!(
                    "continuous assignment references undeclared signal: {reference}"
                ));
            }
        }
    }
    for process in &sequential_processes {
        if !declarations.contains(&process.target) && !known.contains(&process.target) {
            return Err(format!(
                "nonblocking assignment target is undeclared: {}",
                process.target
            ));
        }
        if !known.contains(&process.clock) {
            return Err(format!(
                "clocked process references undeclared clock: {}",
                process.clock
            ));
        }
        for reference in &process.referenced_signals {
            if !declarations.contains(reference) && !known.contains(reference) {
                return Err(format!(
                    "nonblocking assignment references undeclared signal: {reference}"
                ));
            }
        }
    }
    for instance in &instances {
        if instance
            .connections
            .first()
            .is_some_and(|connection| matches!(connection.as_str(), "1'b0" | "1'b1"))
        {
            return Err(format!("{} cannot drive a constant literal", instance.name));
        }
    }
    Ok(RtlModule {
        name,
        parameters,
        ports,
        nets,
        instances,
        assignments,
        sequential_processes,
    })
}

fn parse_bit_name(value: &str) -> Option<(&str, usize)> {
    let (base, suffix) = value.split_once('[')?;
    Some((base, suffix.strip_suffix(']')?.parse().ok()?))
}

pub fn export_structural_verilog(module: &RtlModule) -> String {
    fn identifier(value: &str) -> String {
        let mut characters = value.chars();
        let legal = characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            && characters.all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '$')
            });
        if legal {
            value.into()
        } else {
            format!("\\{value} ")
        }
    }

    fn signal(value: &str) -> String {
        if matches!(value, "1'b0" | "1'b1") {
            return value.into();
        }
        if let Some((base, index)) = parse_bit_name(value) {
            return format!("{}[{index}]", identifier(base));
        }
        identifier(value)
    }

    fn range(value: &Option<RtlRange>) -> String {
        value.as_ref().map_or_else(String::new, |range| {
            let msb = range.msb_expression.as_deref().unwrap_or("");
            let lsb = range.lsb_expression.as_deref().unwrap_or("");
            let msb = if msb.is_empty() {
                range.msb.to_string()
            } else {
                msb.into()
            };
            let lsb = if lsb.is_empty() {
                range.lsb.to_string()
            } else {
                lsb.into()
            };
            format!("[{msb}:{lsb}] ")
        })
    }

    let mut output = format!("module {}", identifier(&module.name));
    if !module.parameters.is_empty() {
        output.push_str(" #(\n");
        for (index, parameter) in module.parameters.iter().enumerate() {
            output.push_str(&format!(
                "  parameter integer {} = {}{}\n",
                identifier(&parameter.name),
                parameter.default_expression,
                if index + 1 == module.parameters.len() {
                    ""
                } else {
                    ","
                },
            ));
        }
        output.push(')');
    }
    output.push_str(" (\n");
    for (index, port) in module.ports.iter().enumerate() {
        let direction = match port.direction {
            RtlPortDirection::Input => "input",
            RtlPortDirection::Output => "output",
        };
        let variable = if port.direction == RtlPortDirection::Output
            && module
                .sequential_processes
                .iter()
                .any(|process| process.target == port.name)
        {
            " logic"
        } else {
            ""
        };
        output.push_str(&format!(
            "  {direction}{variable} {}{}{}\n",
            range(&port.range),
            identifier(&port.name),
            if index + 1 == module.ports.len() {
                ""
            } else {
                ","
            },
        ));
    }
    output.push_str(");\n");
    for net in &module.nets {
        output.push_str(&format!(
            "  wire {}{};\n",
            range(&net.range),
            identifier(&net.name)
        ));
    }
    if !module.nets.is_empty() && !module.instances.is_empty() {
        output.push('\n');
    }
    for instance in &module.instances {
        output.push_str(&format!("  {}", identifier(&instance.cell)));
        if !instance.parameter_overrides.is_empty() {
            let overrides = instance
                .parameter_overrides
                .iter()
                .map(|parameter| {
                    parameter.name.as_ref().map_or_else(
                        || parameter.expression.clone(),
                        |name| format!(".{}({})", identifier(name), parameter.expression),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!(" #({overrides})"));
        }
        output.push_str(&format!(
            " {}({});\n",
            identifier(&instance.name),
            instance
                .connections
                .iter()
                .map(|connection| signal(connection))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    for assignment in &module.assignments {
        output.push_str(&format!(
            "  assign {} = {};\n",
            signal(&assignment.target),
            assignment.expression,
        ));
    }
    for process in &module.sequential_processes {
        let edge = match process.edge {
            RtlEdge::Posedge => "posedge",
            RtlEdge::Negedge => "negedge",
        };
        output.push_str(&format!(
            "  always_ff @({edge} {}) {} <= {};\n",
            signal(&process.clock),
            signal(&process.target),
            process.expression
        ));
    }
    output.push_str("endmodule\n");
    output
}

#[cfg(test)]
mod tests {
    use super::{
        export_structural_verilog, map_module, parse_structural_verilog, PrimitiveGate, RtlEdge,
        RtlPortDirection,
    };

    #[test]
    fn parses_ansi_structural_module_comments_and_assigns() {
        let module = parse_structural_verilog(
            r#"
            // Small structural example
            module half_adder(input A, input B, output SUM, output CARRY);
              wire inverted;
              xor u_sum(SUM, A, B);
              and u_carry(CARRY, A, B);
              assign inverted = ~SUM;
            endmodule
            "#,
        )
        .unwrap();
        assert_eq!(module.name, "half_adder");
        assert_eq!(module.ports[0].direction, RtlPortDirection::Input);
        assert_eq!(module.ports[2].direction, RtlPortDirection::Output);
        assert_eq!(module.nets[0].name, "inverted");
        assert_eq!(module.instances.len(), 3);
        assert_eq!(module.instances[2].primitive, Some(PrimitiveGate::Not));
    }

    #[test]
    fn parses_classic_port_declarations() {
        let module = parse_structural_verilog(
            "module inv(A, Y); input A; output Y; not u0(Y, A); endmodule",
        )
        .unwrap();
        assert_eq!(module.ports.len(), 2);
        assert_eq!(module.instances[0].connections, ["Y", "A"]);
    }

    #[test]
    fn rejects_undeclared_connections_and_behavioral_syntax() {
        assert!(parse_structural_verilog(
            "module bad(input A, output Y); and u0(Y, A, missing); endmodule"
        )
        .unwrap_err()
        .contains("undeclared signal"));
        assert!(
            parse_structural_verilog("module bad(input A, output Y); always Y = A; endmodule")
                .unwrap_err()
                .contains("combinational @* sensitivity")
        );
        assert!(parse_structural_verilog(
            "module bad(A, Y); input A; input A; output Y; buf u0(Y, A); endmodule"
        )
        .unwrap_err()
        .contains("direction declared more than once"));
    }

    #[test]
    fn reports_explicit_later_scope_constructs() {
        assert!(parse_structural_verilog(
            "module n(input A, output Y); buf u0(.Y(Y), .A(A)); endmodule"
        )
        .unwrap_err()
        .contains("named port"));
        assert!(parse_structural_verilog(
            "module a(input A); endmodule module b(input B); endmodule"
        )
        .unwrap_err()
        .contains("only one module"));
    }

    #[test]
    fn parses_packed_vectors_and_constant_bit_selects() {
        let module = parse_structural_verilog(
            "module bits(input [3:0] A, output [1:0] Y); wire [1:0] n; and u0(n[1], A[3], A[2]); not u1(Y[0], n[1]); endmodule",
        )
        .unwrap();
        let input = module.ports.iter().find(|port| port.name == "A").unwrap();
        assert_eq!(input.range.as_ref().unwrap().width(), 4);
        assert_eq!(module.nets[0].range.as_ref().unwrap().width(), 2);
        assert_eq!(module.instances[0].connections, ["n[1]", "A[3]", "A[2]"]);
    }

    #[test]
    fn packed_vectors_reject_ambiguous_widths_and_bad_selects() {
        assert!(parse_structural_verilog(
            "module wide(input [3:0] A, output Y); buf u0(Y, A); endmodule"
        )
        .unwrap_err()
        .contains("4 bits wide"));
        assert!(parse_structural_verilog(
            "module bounds(input [3:0] A, output Y); buf u0(Y, A[4]); endmodule"
        )
        .unwrap_err()
        .contains("outside declared range"));
        assert!(parse_structural_verilog(
            "module slice(input [3:0] A, output Y); buf u0(Y, A[3:2]); endmodule"
        )
        .unwrap_err()
        .contains("part selects"));
    }

    #[test]
    fn parses_scalar_constants_and_explains_procedural_assignment() {
        let module = parse_structural_verilog(
            "module constants(input A, output Y, output Z); and u0(Y, A, 0); or u1(Z, A, 1'b1); endmodule",
        )
        .unwrap();
        assert_eq!(module.instances[0].connections[2], "1'b0");
        assert_eq!(module.instances[1].connections[2], "1'b1");
        let error = parse_structural_verilog(
            "module procedural(input A, output reg Y); always @* Y <= A; endmodule",
        )
        .unwrap_err();
        assert!(error.contains("nonblocking assignment"));
        assert!(error.contains("edge-triggered"));
    }

    #[test]
    fn elaborates_integer_parameters_ranges_and_instance_overrides() {
        let module = parse_structural_verilog(
            "module parametrized #(parameter integer WIDTH = 4, parameter DOUBLE = WIDTH * 2)(input [WIDTH-1:0] A, output Y); external_cell #(.WIDTH(DOUBLE)) u0(Y, A[0]); external_cell #(2) u1(Y, A[1]); endmodule",
        )
        .unwrap();
        assert_eq!(module.parameters[0].default_value, 4);
        assert_eq!(module.parameters[1].default_value, 8);
        assert_eq!(module.ports[0].range.as_ref().unwrap().width(), 4);
        assert_eq!(module.instances[0].cell, "external_cell");
        assert_eq!(module.instances[0].primitive, None);
        assert_eq!(
            module.instances[0].parameter_overrides[0].name.as_deref(),
            Some("WIDTH")
        );
        assert_eq!(module.instances[0].parameter_overrides[0].value, 8);
        assert_eq!(module.instances[1].parameter_overrides[0].name, None);
        assert_eq!(module.instances[1].parameter_overrides[0].value, 2);
    }

    #[test]
    fn parameter_elaboration_rejects_duplicates_cycles_and_invalid_widths() {
        assert!(parse_structural_verilog(
            "module duplicate #(parameter W = 2, parameter W = 3)(input A); endmodule"
        )
        .unwrap_err()
        .contains("more than once"));
        assert!(parse_structural_verilog(
            "module cycle #(parameter A = B, parameter B = A)(input X); endmodule"
        )
        .unwrap_err()
        .contains("cycles"));
        assert!(parse_structural_verilog(
            "module width #(parameter W = 0)(input [W-1:0] A); endmodule"
        )
        .unwrap_err()
        .contains("non-negative"));
        assert!(parse_structural_verilog(
            "module mixed(input A, output Y); cell #(.W(2), 3) u0(Y, A); endmodule"
        )
        .unwrap_err()
        .contains("mix named and positional"));
    }

    #[test]
    fn logical_mapping_is_stable_and_topology_layered() {
        let module = parse_structural_verilog(
            "module chain(input A, input B, output Y); wire n; and u0(n, A, B); not u1(Y, n); endmodule",
        )
        .unwrap();
        let first = map_module(module.clone());
        let second = map_module(module);
        assert_eq!(first, second);
        assert_eq!(first.placements[0].level, 0);
        assert_eq!(first.placements[1].level, 1);
        assert!(first.placements[1].x > first.placements[0].x);
    }

    #[test]
    fn deterministic_export_round_trips_vectors_parameters_and_instances() {
        let source = "module exported #(parameter WIDTH = 4)(input [WIDTH-1:0] A, output Y); wire n; external #(.WIDTH(WIDTH)) child(n, A[0]); and gate(Y, n, 1'b1); endmodule";
        let parsed = parse_structural_verilog(source).unwrap();
        let first = export_structural_verilog(&parsed);
        let second = export_structural_verilog(&parsed);
        assert_eq!(first, second);
        let restored = parse_structural_verilog(&first).unwrap();
        assert_eq!(restored, parsed);
        assert!(first.contains("input [WIDTH-1:0] A"));
        assert!(first.contains("external #(.WIDTH(WIDTH)) child"));
    }

    #[test]
    fn parses_typed_ansi_ports_and_preserves_vector_arithmetic_assignments() {
        let source = r#"
            module adder (
                input wire [3:0] a,
                input wire [3:0] b,
                output wire [4:0] sum
            );
              assign sum = a + b;
            endmodule
        "#;
        let module = parse_structural_verilog(source).unwrap();
        assert_eq!(module.ports[0].name, "a");
        assert_eq!(module.ports[0].range.as_ref().unwrap().width(), 4);
        assert_eq!(module.assignments.len(), 1);
        assert_eq!(module.assignments[0].target, "sum");
        assert_eq!(module.assignments[0].expression, "a+b");
        assert_eq!(module.assignments[0].referenced_signals, ["a", "b"]);
        let exported = export_structural_verilog(&module);
        assert_eq!(parse_structural_verilog(&exported).unwrap(), module);
    }

    #[test]
    fn lowers_unconditional_combinational_always_blocks_to_logical_assignments() {
        let module = parse_structural_verilog(
            "module comb(input logic A, input logic B, output logic Y); always_comb begin Y = A & B; end endmodule",
        ).unwrap();
        assert_eq!(module.assignments.len(), 1);
        assert_eq!(module.assignments[0].target, "Y");
        assert_eq!(module.assignments[0].expression, "A&B");
        assert_eq!(module.assignments[0].referenced_signals, ["A", "B"]);

        let classic = parse_structural_verilog(
            "module comb(input A, input B, output Y); always @(*) Y = A | B; endmodule",
        )
        .unwrap();
        assert_eq!(classic.assignments[0].expression, "A|B");
    }

    #[test]
    fn lowers_definitely_assigned_if_else_to_a_mux_expression() {
        let module = parse_structural_verilog(
            "module mux(input S, input A, input B, output logic Y); always_comb begin if (S) Y = A; else Y = B; end endmodule",
        ).unwrap();
        assert_eq!(module.assignments[0].target, "Y");
        assert_eq!(module.assignments[0].expression, "(S)?(A):(B)");
        assert_eq!(module.assignments[0].referenced_signals, ["S", "A", "B"]);

        let latch = parse_structural_verilog(
            "module latch(input S, input A, output logic Y); always_comb if (S) Y = A; endmodule",
        )
        .unwrap_err();
        assert!(latch.contains("inferred latch"));
    }

    #[test]
    fn lowers_bounded_case_statements_and_requires_default() {
        let module = parse_structural_verilog(
            "module decode(input [1:0] S, input A, input B, output logic Y); always_comb case (S) 2'b00: Y = A; 2'b01: Y = B; default: Y = 0; endcase endmodule",
        ).unwrap();
        assert_eq!(module.assignments[0].target, "Y");
        assert!(module.assignments[0].expression.contains("S)==(2'b00"));
        assert_eq!(module.assignments[0].referenced_signals, ["S", "A", "B"]);

        let latch = parse_structural_verilog(
            "module decode(input S, input A, output logic Y); always_comb case (S) 0: Y = A; endcase endmodule",
        ).unwrap_err();
        assert!(latch.contains("default item") && latch.contains("inferred latch"));
    }

    #[test]
    fn parses_and_round_trips_edge_triggered_nonblocking_assignments() {
        let source = "module dff(input logic CLK, input logic D, output logic Q); always_ff @(posedge CLK) Q <= D; endmodule";
        let module = parse_structural_verilog(source).unwrap();
        assert_eq!(module.sequential_processes.len(), 1);
        assert_eq!(module.sequential_processes[0].edge, RtlEdge::Posedge);
        assert_eq!(module.sequential_processes[0].clock, "CLK");
        assert_eq!(module.sequential_processes[0].target, "Q");
        assert_eq!(module.sequential_processes[0].expression, "D");
        assert_eq!(
            parse_structural_verilog(&export_structural_verilog(&module)).unwrap(),
            module
        );

        let classic = parse_structural_verilog(
            "module dff(input CLK, input D, output reg Q); always @(negedge CLK) begin Q <= D; end endmodule",
        ).unwrap();
        assert_eq!(classic.sequential_processes[0].edge, RtlEdge::Negedge);
    }

    #[test]
    fn accepts_quoted_attributes_and_preserves_braced_assignment_expressions() {
        let source = r#"
            module packed #(parameter integer WIDTH = 4) (
              input A, input B, output [1:0] Y, output [3:0] ZERO
            );
              (* ram_style = "block" *) wire ignored_attribute_target;
              assign Y = {A, B};
              assign ZERO = {WIDTH{1'b0}};
            endmodule
        "#;
        let module = parse_structural_verilog(source).unwrap();
        assert_eq!(module.assignments[0].expression, "{A,B}");
        assert_eq!(module.assignments[0].referenced_signals, ["A", "B"]);
        assert_eq!(module.assignments[1].expression, "{WIDTH{1'b0}}");
        assert!(module.assignments[1].referenced_signals.is_empty());
        assert_eq!(
            parse_structural_verilog(&export_structural_verilog(&module)).unwrap(),
            module
        );
    }
}

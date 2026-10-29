use std::collections::HashMap;

use crate::frontend::ast::{
    AccessStep, BinaryOperator, Expression, FunctionCallExpression, LiteralType, UnaryOperator,
};

use super::names::iec_ident;
use super::type_codegen::raw_cpp_type;
use super::type_registry::TypeRegistry;

#[derive(Debug, Clone, Default)]
pub struct ExpressionContext {
    pub function_name: Option<String>,
    pub current_fb_name: Option<String>,
    pub current_fb_extends: Option<String>,
    pub external_scalars: HashMap<String, String>,
    pub external_composites: HashMap<String, String>,
    pub function_block_instances: HashMap<String, String>,
    pub variable_cpp_names: HashMap<String, String>,
    pub variable_types: HashMap<String, String>,
    pub types: TypeRegistry,
}

pub fn emit_expression(
    expression: &Expression,
    context: &ExpressionContext,
) -> Result<String, String> {
    match expression {
        Expression::Literal(literal) => emit_literal(literal.literal_type, &literal.raw_value),
        Expression::Variable(variable) => {
            let name = iec_ident(&variable.name);
            let is_known_variable = context.variable_types.contains_key(&name)
                || context.external_scalars.contains_key(&name)
                || context.external_composites.contains_key(&name)
                || context.function_block_instances.contains_key(&name);

            if !is_known_variable && variable.access_chain.is_empty() {
                if let Some(owner) = context.types.enum_member_owner(&name) {
                    return Ok(format!("{owner}::{name}"));
                }
            }
            if context.types.is_enum(&name) && variable.access_chain.len() == 1 {
                if let AccessStep::Field(member) = &variable.access_chain[0] {
                    return Ok(format!("{name}::{}", iec_ident(member)));
                }
            }

            let current_type = context.variable_types.get(&name).cloned();
            if let Some(type_name) = context.external_composites.get(&name) {
                let pointer = context
                    .variable_cpp_names
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| name.clone());
                let inner = render_access_tail(
                    "(*__glk)".to_owned(),
                    Some(type_name.clone()),
                    &variable.access_chain,
                    context,
                )?;
                return Ok(format!(
                    "{pointer}->with_lock([&](auto* __glk){{ return {inner}; }})"
                ));
            }

            let output = if context.function_name.as_deref() == Some(name.as_str()) {
                format!("{name}_result")
            } else if variable.access_chain.is_empty()
                && context.external_scalars.contains_key(&name)
            {
                format!("{name}->read()")
            } else {
                context
                    .variable_cpp_names
                    .get(&name)
                    .cloned()
                    .unwrap_or(name)
            };

            render_access_tail(output, current_type, &variable.access_chain, context)
        }
        Expression::Binary {
            operator,
            left,
            right,
            ..
        } => {
            let left = emit_expression(left, context)?;
            let right = emit_expression(right, context)?;
            if *operator == BinaryOperator::Power {
                return Ok(format!(
                    "std::pow(static_cast<double>({left}), static_cast<double>({right}))"
                ));
            }
            let op = binary_operator(*operator);
            if matches!(
                operator,
                BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor
            ) {
                Ok(format!("({left}) {op} ({right})"))
            } else {
                Ok(format!("{left} {op} {right}"))
            }
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand = emit_expression(operand, context)?;
            Ok(match operator {
                UnaryOperator::Not => format!("!{operand}"),
                UnaryOperator::Negate => format!("-{operand}"),
                UnaryOperator::Positive => format!("+{operand}"),
            })
        }
        Expression::Parenthesized { expression, .. } => {
            Ok(format!("({})", emit_expression(expression, context)?))
        }
        Expression::FunctionCall(call) => emit_call(call, context),
        Expression::New {
            allocation_type,
            array_size,
            ..
        } => {
            let cpp_type = raw_cpp_type(&allocation_type.name);
            if let Some(array_size) = array_size {
                Ok(format!(
                    "strucpp::iec_new_array<{cpp_type}>({})",
                    emit_expression(array_size, context)?
                ))
            } else {
                Ok(format!("strucpp::iec_new<{cpp_type}>()"))
            }
        }
        Expression::ArrayLiteral { elements, .. } => {
            let values = elements
                .iter()
                .map(|element| emit_expression(element, context))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{{{}}}", values.join(", ")))
        }
    }
}

pub(crate) fn render_access_tail(
    mut output: String,
    mut current_type: Option<String>,
    access_chain: &[AccessStep],
    context: &ExpressionContext,
) -> Result<String, String> {
    for step in access_chain {
        match step {
            AccessStep::Field(field) => {
                if field.chars().all(|ch| ch.is_ascii_digit()) {
                    output = format!("(({output} >> {field}) & 1)");
                    current_type = None;
                    continue;
                }
                let field_key = iec_ident(field);
                if let Some(type_name) = current_type.as_deref() {
                    if let Some(info) = context.types.field(type_name, &field_key) {
                        output.push('.');
                        output.push_str(&info.cpp_name);
                        current_type = if info.is_array {
                            Some(format!(
                                "@ARRAY:{}",
                                info.array_element_type.as_deref().unwrap_or_default()
                            ))
                        } else {
                            Some(info.type_name.clone())
                        };
                        continue;
                    }
                }
                output.push('.');
                output.push_str(&field_key);
                current_type = None;
            }
            AccessStep::Subscript(indices) => {
                let rendered = indices
                    .iter()
                    .map(|index| emit_expression(index, context))
                    .collect::<Result<Vec<_>, _>>()?;
                let inline_element = current_type
                    .as_deref()
                    .and_then(|type_name| type_name.strip_prefix("@ARRAY:"))
                    .map(str::to_owned);
                let named_element = current_type
                    .as_deref()
                    .filter(|type_name| context.types.is_array(type_name))
                    .and_then(|type_name| context.types.array_element_type(type_name))
                    .map(str::to_owned);
                if let Some(element_type) = inline_element.or(named_element) {
                    output.push_str(".at(");
                    output.push_str(&rendered.join(", "));
                    output.push(')');
                    current_type = Some(element_type);
                } else {
                    for index in rendered {
                        output.push('[');
                        output.push_str(&index);
                        output.push(']');
                    }
                    current_type = None;
                }
            }
            AccessStep::Dereference => output.push_str(".deref()"),
        }
    }
    Ok(output)
}

pub fn emit_call(
    call: &FunctionCallExpression,
    context: &ExpressionContext,
) -> Result<String, String> {
    let name = iec_ident(&call.function_name);
    let arguments = call
        .arguments
        .iter()
        .filter(|argument| {
            !argument.name.as_deref().is_some_and(|name| {
                name.eq_ignore_ascii_case("EN") || name.eq_ignore_ascii_case("ENO")
            })
        })
        .map(|argument| emit_expression(&argument.value, context))
        .collect::<Result<Vec<_>, _>>()?;

    if let Some((prefix, method)) = name.split_once('.') {
        let method = iec_ident(method);
        return Ok(match prefix {
            "THIS" => format!("this->{method}({})", arguments.join(", ")),
            "SUPER" => {
                let parent = context.current_fb_extends.as_deref().ok_or_else(|| {
                    "SUPER method call outside inherited function block".to_owned()
                })?;
                format!("{parent}::{method}({})", arguments.join(", "))
            }
            _ => format!("{}.{}({})", iec_ident(prefix), method, arguments.join(", ")),
        });
    }

    if name == "SUPER" {
        let parent = context
            .current_fb_extends
            .as_deref()
            .ok_or_else(|| "SUPER^() outside inherited function block".to_owned())?;
        return Ok(format!("{parent}::operator()({})", arguments.join(", ")));
    }

    if matches!(name.as_str(), "LOWER_BOUND" | "UPPER_BOUND") {
        let array = arguments
            .first()
            .ok_or_else(|| format!("{name} requires an array argument"))?;
        let method = if name == "LOWER_BOUND" {
            "lower_bound"
        } else {
            "upper_bound"
        };
        return Ok(if let Some(dimension) = arguments.get(1) {
            format!("{array}.{method}({dimension})")
        } else {
            format!("{array}.{method}()")
        });
    }

    if name == "ADR" {
        let first = call
            .arguments
            .first()
            .ok_or_else(|| "ADR requires one argument".to_owned())?;
        return Ok(format!("&({})", emit_expression(&first.value, context)?));
    }
    Ok(format!("{name}({})", arguments.join(", ")))
}

fn emit_literal(literal_type: LiteralType, raw: &str) -> Result<String, String> {
    Ok(match literal_type {
        LiteralType::Bool => {
            if raw.eq_ignore_ascii_case("TRUE") {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        LiteralType::Int => integer_literal(raw)?,
        LiteralType::Real => raw.replace('_', ""),
        LiteralType::String => {
            let inner = raw
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
                .unwrap_or(raw);
            format!("\"{}\"", inner.replace("''", "'").replace('"', "\\\""))
        }
        LiteralType::WString => {
            let inner = raw
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(raw);
            format!("u\"{}\"", inner.replace('"', "\\\""))
        }
        LiteralType::Time => format!("{}LL", parse_time_ns(raw)?),
        LiteralType::Date => format!("{}LL", parse_date_days(raw)),
        LiteralType::TimeOfDay => format!("{}LL", parse_tod_ns(raw)),
        LiteralType::DateAndTime => format!("{}LL", parse_datetime_ns(raw)),
        LiteralType::Null => "nullptr".to_owned(),
        LiteralType::Typed => {
            let (prefix, value) = raw
                .split_once('#')
                .ok_or_else(|| format!("invalid typed literal {raw:?}"))?;
            format!(
                "static_cast<IEC_{}>({})",
                iec_ident(prefix),
                integer_literal(value)?
            )
        }
    })
}

fn integer_literal(raw: &str) -> Result<String, String> {
    let raw = raw.replace('_', "");
    if let Some(value) = raw.strip_prefix("16#") {
        return Ok(format!("0x{value}"));
    }
    if let Some(value) = raw.strip_prefix("8#") {
        return Ok(format!("0{value}"));
    }
    if let Some(value) = raw.strip_prefix("2#") {
        let parsed = u128::from_str_radix(value, 2)
            .map_err(|_| format!("invalid binary integer literal {raw:?}"))?;
        return Ok(parsed.to_string());
    }
    Ok(raw)
}

fn parse_time_ns(raw: &str) -> Result<i64, String> {
    let (_, body) = raw
        .split_once('#')
        .ok_or_else(|| format!("invalid TIME literal {raw:?}"))?;
    let mut rest = body;
    let mut total = 0f64;
    while !rest.is_empty() {
        let number_len = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '_' || *ch == '.')
            .map(|(index, ch)| index + ch.len_utf8())
            .last()
            .unwrap_or(0);
        if number_len == 0 {
            return Err(format!("invalid TIME literal component {rest:?}"));
        }
        let value: f64 = rest[..number_len]
            .replace('_', "")
            .parse()
            .map_err(|_| format!("invalid TIME number in {raw:?}"))?;
        rest = &rest[number_len..];
        let (factor, unit_len) = if rest.len() >= 2 && rest[..2].eq_ignore_ascii_case("ms") {
            (1_000_000f64, 2)
        } else if rest.len() >= 2 && rest[..2].eq_ignore_ascii_case("us") {
            (1_000f64, 2)
        } else if rest.len() >= 2 && rest[..2].eq_ignore_ascii_case("ns") {
            (1f64, 2)
        } else {
            let unit = rest
                .chars()
                .next()
                .ok_or_else(|| format!("TIME unit missing in {raw:?}"))?;
            let factor = match unit.to_ascii_lowercase() {
                'd' => 86_400_000_000_000f64,
                'h' => 3_600_000_000_000f64,
                'm' => 60_000_000_000f64,
                's' => 1_000_000_000f64,
                _ => return Err(format!("unknown TIME unit {unit:?}")),
            };
            (factor, unit.len_utf8())
        };
        total += value * factor;
        rest = &rest[unit_len..];
    }
    Ok(total.round() as i64)
}

fn parse_date_days(raw: &str) -> i64 {
    let Some(body) = strip_temporal_prefix(raw, &["D#", "DATE#"]) else {
        return 0;
    };
    let parts = body.split('-').collect::<Vec<_>>();
    if parts.len() != 3 {
        return 0;
    }
    let Some(year) = parse_i64(parts[0]) else {
        return 0;
    };
    let Some(month) = parse_i64(parts[1]) else {
        return 0;
    };
    let Some(day) = parse_i64(parts[2]) else {
        return 0;
    };
    js_utc_days(year, month, day)
}

fn parse_tod_ns(raw: &str) -> i128 {
    let Some(body) = strip_temporal_prefix(raw, &["TOD#", "TIME_OF_DAY#"]) else {
        return 0;
    };
    let Some((hour, minute, second, fraction)) = parse_clock(body) else {
        return 0;
    };
    i128::from(hour) * 3_600_000_000_000
        + i128::from(minute) * 60_000_000_000
        + i128::from(second) * 1_000_000_000
        + fraction
}

fn parse_datetime_ns(raw: &str) -> i128 {
    let Some(body) = strip_temporal_prefix(raw, &["DT#", "DATE_AND_TIME#"]) else {
        return 0;
    };
    let parts = body.splitn(4, '-').collect::<Vec<_>>();
    if parts.len() != 4 {
        return 0;
    }
    let Some(year) = parse_i64(parts[0]) else {
        return 0;
    };
    let Some(month) = parse_i64(parts[1]) else {
        return 0;
    };
    let Some(day) = parse_i64(parts[2]) else {
        return 0;
    };
    let Some((hour, minute, second, fraction)) = parse_clock(parts[3]) else {
        return 0;
    };
    let days = i128::from(js_utc_days(year, month, day));
    days * 86_400_000_000_000
        + i128::from(hour) * 3_600_000_000_000
        + i128::from(minute) * 60_000_000_000
        + i128::from(second) * 1_000_000_000
        + fraction
}

fn strip_temporal_prefix<'a>(raw: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|prefix| {
        raw.get(..prefix.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
            .then(|| &raw[prefix.len()..])
    })
}

fn parse_clock(value: &str) -> Option<(i64, i64, i64, i128)> {
    let parts = value.split(':').collect::<Vec<_>>();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let hour = parse_i64(parts[0])?;
    let minute = parse_i64(parts[1])?;
    let (second, fraction) = if let Some(second) = parts.get(2) {
        if let Some((whole, fraction)) = second.split_once('.') {
            (parse_i64(whole)?, fraction_to_ns(fraction)?)
        } else {
            (parse_i64(second)?, 0)
        }
    } else {
        (0, 0)
    };
    Some((hour, minute, second, fraction))
}

fn fraction_to_ns(value: &str) -> Option<i128> {
    if !value.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let digits = value.as_bytes();
    let take = digits.len().min(9);
    let prefix = std::str::from_utf8(&digits[..take]).ok()?;
    let mut result = prefix.parse::<i128>().ok()?;
    for _ in take..9 {
        result *= 10;
    }
    Some(result)
}

fn parse_i64(value: &str) -> Option<i64> {
    (!value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn js_utc_days(mut year: i64, month_one_based: i64, day: i64) -> i64 {
    // ECMAScript Date.UTC maps years 0..99 to 1900..1999 before normalizing.
    if (0..=99).contains(&year) {
        year += 1900;
    }
    let month_zero = month_one_based - 1;
    let year_delta = month_zero.div_euclid(12);
    let normalized_month_zero = month_zero.rem_euclid(12);
    year += year_delta;
    let month = normalized_month_zero + 1;
    days_from_civil(year, month, 1) + day - 1
}

fn days_from_civil(mut year: i64, month: i64, day: i64) -> i64 {
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 }.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn binary_operator(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Mod => "%",
        BinaryOperator::Power => "**",
        BinaryOperator::And => "&",
        BinaryOperator::Or => "|",
        BinaryOperator::Xor => "^",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Less => "<",
        BinaryOperator::Greater => ">",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::GreaterEqual => ">=",
    }
}

#[cfg(test)]
mod tests {
    use crate::frontend::parse;

    use super::*;

    #[test]
    fn emits_openplc_expression_precedence_shape() {
        let ast = parse(
            "PROGRAM main VAR x,a,b,c,d : INT; END_VAR x := a + b * c > d AND TRUE; END_PROGRAM",
            "expr.st",
        )
        .unwrap();
        let crate::frontend::ast::Statement::Assignment(statement) = &ast.programs[0].body[0]
        else {
            panic!("assignment")
        };
        assert_eq!(
            emit_expression(&statement.value, &ExpressionContext::default()).unwrap(),
            "(A + B * C > D) & (true)"
        );
    }

    #[test]
    fn time_literal_is_nanoseconds() {
        assert_eq!(parse_time_ns("T#20ms").unwrap(), 20_000_000);
        assert_eq!(parse_time_ns("TIME#1h2m3s").unwrap(), 3_723_000_000_000);
    }

    #[test]
    fn date_tod_and_datetime_match_pinned_strucpp_epoch_values() {
        assert_eq!(parse_date_days("D#1970-01-02"), 1);
        assert_eq!(parse_tod_ns("TOD#01:02:03.000000004"), 3_723_000_000_004);
        assert_eq!(
            parse_datetime_ns("DT#1970-01-02-01:02:03.000000004"),
            90_123_000_000_004
        );
    }
}

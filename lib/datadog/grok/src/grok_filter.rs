use crate::ast::{Function, FunctionArgument};
use crate::parse_grok::Error as GrokRuntimeError;
use crate::parse_grok_rules::Error as GrokStaticError;
use ordered_float::NotNan;
use percent_encoding::percent_decode;
use regex::Regex;
use std::{convert::TryFrom, ops::Deref, string::ToString};
use strum_macros::Display;
use tracing::error;
use vrl_compiler::Value;

#[derive(Debug, Display, Clone)]
pub enum GrokFilter {
    Integer,
    IntegerExt,
    Number,
    NumberExt,
    NullIf(String),
    Scale(f64),
    Lowercase,
    Uppercase,
    Json,
}

impl TryFrom<&Function> for GrokFilter {
    type Error = GrokStaticError;

    fn try_from(f: &Function) -> Result<Self, Self::Error> {
        match f.name.as_str() {
            "scale" => match f.args.as_ref() {
                Some(args) if !args.is_empty() => {
                    let scale_factor = match args[0] {
                        FunctionArgument::Arg(Value::Integer(scale_factor)) => scale_factor as f64,
                        FunctionArgument::Arg(Value::Float(scale_factor)) => {
                            scale_factor.into_inner()
                        }
                        _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
                    };
                    Ok(GrokFilter::Scale(scale_factor))
                }
                _ => Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            },
            "integer" => Ok(GrokFilter::Integer),
            "integerExt" => Ok(GrokFilter::IntegerExt),
            "number" => Ok(GrokFilter::Number),
            "numberExt" => Ok(GrokFilter::NumberExt),
            "lowercase" => Ok(GrokFilter::Lowercase),
            "uppercase" => Ok(GrokFilter::Uppercase),
            "json" => Ok(GrokFilter::Json),
            "nullIf" => f
                .args
                .as_ref()
                .and_then(|args| {
                    if let FunctionArgument::Arg(ref null_value) = args[0] {
                        return Some(GrokFilter::NullIf(null_value.to_string()));
                    } else {
                        None
                    }
                })
                .ok_or_else(|| GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            _ => Err(GrokStaticError::UnknownFilter(f.name.clone())),
        }
    }
}

pub fn apply_filter(value: &Value, filter: &GrokFilter) -> Result<Value, GrokRuntimeError> {
    match filter {
        GrokFilter::Integer => match value {
            Value::Bytes(v) => Ok(String::from_utf8_lossy(v)
                .parse::<i64>()
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })?
                .into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::IntegerExt => match value {
            Value::Bytes(v) => Ok(String::from_utf8_lossy(v)
                .parse::<f64>()
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })
                .map(|f| (f as i64).into())
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })?),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Number | GrokFilter::NumberExt => match value {
            Value::Bytes(v) => Ok(String::from_utf8_lossy(v)
                .parse::<f64>()
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })?
                .into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Scale(scale_factor) => match value {
            Value::Integer(v) => Ok(Value::Float(
                NotNan::new((*v as f64) * scale_factor).expect("NaN"),
            )),
            Value::Float(v) => Ok(Value::Float(*v * scale_factor)),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Lowercase => match value {
            Value::Bytes(bytes) => Ok(String::from_utf8_lossy(&bytes).to_lowercase().into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Uppercase => match value {
            Value::Bytes(bytes) => Ok(String::from_utf8_lossy(&bytes).to_uppercase().into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::Json => match value {
            Value::Bytes(bytes) => serde_json::from_slice::<'_, serde_json::Value>(bytes.as_ref())
                .map_err(|_e| {
                    GrokRuntimeError::FailedToApplyFilter(filter.to_string(), value.to_string())
                })
                .map(|v| v.into()),
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
        GrokFilter::NullIf(null_value) => match value {
            Value::Bytes(_) => {
                if value.to_string() == *null_value {
                    Ok(Value::Null)
                } else {
                    Ok(value.to_owned())
                }
            }
            _ => Err(GrokRuntimeError::FailedToApplyFilter(
                filter.to_string(),
                value.to_string(),
            )),
        },
    }
}

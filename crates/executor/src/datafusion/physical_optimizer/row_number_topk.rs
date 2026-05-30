use std::sync::Arc;

use crate::datafusion::physical_plan::grouped_topk::GroupedTopKExec;
use datafusion::arrow::datatypes::DataType;
use datafusion::physical_expr::ScalarFunctionExpr;
use datafusion::physical_expr::expressions::{BinaryExpr, CastExpr, Column, Literal};
use datafusion::physical_expr::{PhysicalExpr, PhysicalSortExpr};
use datafusion_common::config::ConfigOptions;
use datafusion_common::tree_node::{Transformed, TransformedResult, TreeNode};
use datafusion_common::{Result as DFResult, ScalarValue};
use datafusion_expr::Operator;
use datafusion_physical_plan::filter::FilterExec;
use datafusion_physical_plan::projection::ProjectionExec;
use datafusion_physical_plan::windows::{BoundedWindowAggExec, WindowAggExec};
use datafusion_physical_plan::{ExecutionPlan, WindowExpr};

#[derive(Debug, Default)]
pub struct RowNumberTopK;

impl RowNumberTopK {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl datafusion::physical_optimizer::PhysicalOptimizerRule for RowNumberTopK {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> DFResult<Arc<dyn ExecutionPlan>> {
        plan.transform_up(|plan| {
            let Some(filter) = plan.as_any().downcast_ref::<FilterExec>() else {
                return Ok(Transformed::no(plan));
            };

            let Some(replacement) = replacement_filter_input(filter)? else {
                return Ok(Transformed::no(plan));
            };

            plan.with_new_children(vec![replacement])
                .map(Transformed::yes)
        })
        .data()
    }

    fn name(&self) -> &'static str {
        "RowNumberTopK"
    }

    fn schema_check(&self) -> bool {
        true
    }
}

struct WindowPlan<'a> {
    input: &'a Arc<dyn ExecutionPlan>,
    window_expr: &'a [Arc<dyn WindowExpr>],
    schema: datafusion::arrow::datatypes::SchemaRef,
}

fn replacement_filter_input(filter: &FilterExec) -> DFResult<Option<Arc<dyn ExecutionPlan>>> {
    if let Some(window) = window_plan(filter.input()) {
        let row_number_index = window.input.schema().fields().len();
        let Some(limit) = predicate_row_number_limit(filter.predicate(), row_number_index) else {
            return Ok(None);
        };
        return build_topk(window, limit);
    }

    let Some(projection) = filter.input().as_any().downcast_ref::<ProjectionExec>() else {
        return Ok(None);
    };
    let Some(window) = window_plan(projection.input()) else {
        return Ok(None);
    };

    let window_row_number_index = window.input.schema().fields().len();
    let Some(projected_row_number_index) = projection
        .expr()
        .iter()
        .position(|expr| is_row_number_column(&expr.expr, window_row_number_index))
    else {
        return Ok(None);
    };

    let Some(limit) = predicate_row_number_limit(filter.predicate(), projected_row_number_index)
    else {
        return Ok(None);
    };

    let Some(topk) = build_topk(window, limit)? else {
        return Ok(None);
    };

    filter
        .input()
        .clone()
        .with_new_children(vec![topk])
        .map(Some)
}

fn window_plan(plan: &Arc<dyn ExecutionPlan>) -> Option<WindowPlan<'_>> {
    if let Some(window) = plan.as_any().downcast_ref::<BoundedWindowAggExec>() {
        return Some(WindowPlan {
            input: window.input(),
            window_expr: window.window_expr(),
            schema: window.schema(),
        });
    }

    plan.as_any()
        .downcast_ref::<WindowAggExec>()
        .map(|window| WindowPlan {
            input: window.input(),
            window_expr: window.window_expr(),
            schema: window.schema(),
        })
}

fn build_topk(window: WindowPlan<'_>, limit: usize) -> DFResult<Option<Arc<dyn ExecutionPlan>>> {
    if window.window_expr.len() != 1 {
        return Ok(None);
    }

    let window_expr = &window.window_expr[0];
    if !is_row_number_window(window_expr.name())
        || !window_expr.expressions().is_empty()
        || window_expr.partition_by().is_empty()
        || window_expr.order_by().is_empty()
    {
        return Ok(None);
    }

    let input_schema = window.input.schema();
    let row_number_index = input_schema.fields().len();
    if window.schema.fields().len() != row_number_index + 1 {
        return Ok(None);
    }

    if !all_types_supported(
        window_expr.partition_by(),
        window_expr.order_by(),
        &input_schema,
    )? {
        return Ok(None);
    }

    let topk = GroupedTopKExec::try_new(
        Arc::clone(window.input),
        window_expr.partition_by().to_vec(),
        window_expr.order_by().to_vec(),
        limit,
        window.schema,
    )?;

    Ok(Some(Arc::new(topk)))
}

fn is_row_number_window(name: &str) -> bool {
    name.eq_ignore_ascii_case("row_number")
        || name
            .get(..12)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("row_number()"))
}

fn predicate_row_number_limit(
    predicate: &Arc<dyn PhysicalExpr>,
    row_number_index: usize,
) -> Option<usize> {
    let binary = predicate.as_any().downcast_ref::<BinaryExpr>()?;

    let left_is_row_number = is_row_number_column(binary.left(), row_number_index);
    let right_is_row_number = is_row_number_column(binary.right(), row_number_index);

    if left_is_row_number && !right_is_row_number {
        return limit_from_row_number_cmp_literal(
            *binary.op(),
            literal_positive_usize(binary.right())?,
        );
    }

    if right_is_row_number && !left_is_row_number {
        return limit_from_literal_cmp_row_number(
            *binary.op(),
            literal_positive_usize(binary.left())?,
        );
    }

    None
}

fn is_row_number_column(expr: &Arc<dyn PhysicalExpr>, row_number_index: usize) -> bool {
    if let Some(column) = expr.as_any().downcast_ref::<Column>() {
        return column.index() == row_number_index;
    }

    expr.as_any()
        .downcast_ref::<CastExpr>()
        .is_some_and(|cast| is_row_number_column(cast.expr(), row_number_index))
        || expr
            .as_any()
            .downcast_ref::<ScalarFunctionExpr>()
            .is_some_and(|func| {
                func.name().eq_ignore_ascii_case("to_decimal")
                    && func
                        .args()
                        .first()
                        .is_some_and(|arg| is_row_number_column(arg, row_number_index))
                    && func
                        .args()
                        .iter()
                        .skip(1)
                        .all(|arg| arg.as_any().downcast_ref::<Literal>().is_some())
            })
}

fn literal_positive_usize(expr: &Arc<dyn PhysicalExpr>) -> Option<usize> {
    let value = if let Some(literal) = expr.as_any().downcast_ref::<Literal>() {
        literal.value()
    } else if let Some(cast) = expr.as_any().downcast_ref::<CastExpr>() {
        return literal_positive_usize(cast.expr());
    } else {
        return None;
    };

    scalar_positive_usize(value)
}

fn limit_from_row_number_cmp_literal(op: Operator, literal: usize) -> Option<usize> {
    match op {
        Operator::Eq | Operator::LtEq => Some(literal),
        Operator::Lt => literal.checked_sub(1).filter(|limit| *limit > 0),
        _ => None,
    }
}

fn limit_from_literal_cmp_row_number(op: Operator, literal: usize) -> Option<usize> {
    match op {
        Operator::Eq | Operator::GtEq => Some(literal),
        Operator::Gt => literal.checked_sub(1).filter(|limit| *limit > 0),
        _ => None,
    }
}

fn scalar_positive_usize(value: &ScalarValue) -> Option<usize> {
    let value = match value {
        ScalarValue::Int8(Some(value)) => u64::try_from(*value).ok()?,
        ScalarValue::Int16(Some(value)) => u64::try_from(*value).ok()?,
        ScalarValue::Int32(Some(value)) | ScalarValue::Decimal32(Some(value), _, 0) => {
            u64::try_from(*value).ok()?
        }
        ScalarValue::Int64(Some(value)) | ScalarValue::Decimal64(Some(value), _, 0) => {
            u64::try_from(*value).ok()?
        }
        ScalarValue::UInt8(Some(value)) => u64::from(*value),
        ScalarValue::UInt16(Some(value)) => u64::from(*value),
        ScalarValue::UInt32(Some(value)) => u64::from(*value),
        ScalarValue::UInt64(Some(value)) => *value,
        ScalarValue::Decimal128(Some(value), _, 0) => u64::try_from(*value).ok()?,
        _ => return None,
    };

    usize::try_from(value).ok().filter(|value| *value > 0)
}

fn all_types_supported(
    partition_by: &[Arc<dyn PhysicalExpr>],
    order_by: &[PhysicalSortExpr],
    input_schema: &datafusion::arrow::datatypes::Schema,
) -> DFResult<bool> {
    for expr in partition_by {
        if !is_supported_type(&expr.data_type(input_schema)?) {
            return Ok(false);
        }
    }

    for expr in order_by {
        if !is_supported_type(&expr.expr.data_type(input_schema)?) {
            return Ok(false);
        }
    }

    Ok(true)
}

const fn is_supported_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Null
            | DataType::Boolean
            | DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Utf8
            | DataType::LargeUtf8
            | DataType::Utf8View
            | DataType::Binary
            | DataType::LargeBinary
            | DataType::BinaryView
            | DataType::Date32
            | DataType::Date64
            | DataType::Time32(_)
            | DataType::Time64(_)
            | DataType::Timestamp(_, _)
            | DataType::Duration(_)
            | DataType::Decimal32(_, _)
            | DataType::Decimal64(_, _)
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _)
    )
}

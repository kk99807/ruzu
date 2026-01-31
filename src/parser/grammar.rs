//! Pest parser integration for Cypher grammar.

use pest::Parser;
use pest_derive::Parser;

use crate::error::{Result, RuzuError};
use crate::parser::ast::{AstAggregateFunction, ComparisonOp, CopyOptions, Expression, Literal, NodeFilter, OrderByItem, ReturnItem, Statement};

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
struct CypherParser;

/// Parses a Cypher query string into a Statement AST.
///
/// # Errors
///
/// Returns a `ParseError` if the query is syntactically invalid.
pub fn parse_query(query: &str) -> Result<Statement> {
    let pairs = CypherParser::parse(Rule::cypher_query, query).map_err(|e| {
        let (line, col) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c))
            | pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        RuzuError::ParseError {
            line,
            col,
            message: e.variant.message().to_string(),
        }
    })?;

    build_ast(pairs)
}

fn build_ast(pairs: pest::iterators::Pairs<Rule>) -> Result<Statement> {
    for pair in pairs {
        if pair.as_rule() == Rule::cypher_query {
            for inner in pair.into_inner() {
                if inner.as_rule() == Rule::statement {
                    return build_statement(inner);
                }
            }
        }
    }
    Err(RuzuError::ParseError {
        line: 0,
        col: 0,
        message: "No statement found".into(),
    })
}

fn build_statement(pair: pest::iterators::Pair<Rule>) -> Result<Statement> {
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::explain_query => return build_explain_query(inner),
            Rule::copy_from => return Ok(build_copy_from(inner)),
            Rule::create_node_table => return Ok(build_create_node_table(inner)),
            Rule::create_rel_table => return Ok(build_create_rel_table(inner)),
            Rule::create_node => return build_create_node(inner),
            Rule::match_create => return build_match_create(inner),
            Rule::match_query => return build_match_query(inner),
            _ => {}
        }
    }
    Err(RuzuError::ParseError {
        line: 0,
        col: 0,
        message: "Unknown statement type".into(),
    })
}

fn build_explain_query(pair: pest::iterators::Pair<Rule>) -> Result<Statement> {
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::match_query {
            let inner_stmt = build_match_query(inner)?;
            return Ok(Statement::Explain {
                inner: Box::new(inner_stmt),
            });
        }
    }
    Err(RuzuError::ParseError {
        line: 0,
        col: 0,
        message: "EXPLAIN requires a query".into(),
    })
}

fn build_copy_from(pair: pest::iterators::Pair<Rule>) -> Statement {
    let mut table_name = String::new();
    let mut file_path = String::new();
    let mut options = CopyOptions::default();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                table_name = inner.as_str().to_string();
            }
            Rule::file_path => {
                // Remove surrounding quotes
                let s = inner.as_str();
                file_path = s[1..s.len() - 1].to_string();
            }
            Rule::copy_options => {
                for opt_inner in inner.into_inner() {
                    if opt_inner.as_rule() == Rule::copy_option {
                        for opt in opt_inner.into_inner() {
                            match opt.as_rule() {
                                Rule::copy_option_header => {
                                    for val in opt.into_inner() {
                                        if val.as_rule() == Rule::bool_literal {
                                            options.has_header =
                                                Some(parse_bool_literal(val.as_str()));
                                        }
                                    }
                                }
                                Rule::copy_option_delim => {
                                    for val in opt.into_inner() {
                                        if val.as_rule() == Rule::string_literal {
                                            let s = val.as_str();
                                            let delim_str = &s[1..s.len() - 1];
                                            if let Some(c) = delim_str.chars().next() {
                                                options.delimiter = Some(c);
                                            }
                                        }
                                    }
                                }
                                Rule::copy_option_skip => {
                                    for val in opt.into_inner() {
                                        if val.as_rule() == Rule::integer_literal {
                                            if let Ok(n) = val.as_str().parse::<u32>() {
                                                options.skip_rows = Some(n);
                                            }
                                        }
                                    }
                                }
                                Rule::copy_option_ignore_errors => {
                                    for val in opt.into_inner() {
                                        if val.as_rule() == Rule::bool_literal {
                                            options.ignore_errors =
                                                Some(parse_bool_literal(val.as_str()));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Statement::Copy {
        table_name,
        file_path,
        options,
    }
}

fn parse_bool_literal(s: &str) -> bool {
    s.eq_ignore_ascii_case("true")
}

fn build_create_node_table(pair: pest::iterators::Pair<Rule>) -> Statement {
    let mut table_name = String::new();
    let mut columns = Vec::new();
    let mut primary_key = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                if table_name.is_empty() {
                    table_name = inner.as_str().to_string();
                }
            }
            Rule::column_list => {
                for col_pair in inner.into_inner() {
                    if col_pair.as_rule() == Rule::column_def {
                        let mut parts = col_pair.into_inner();
                        let name = parts.next().unwrap().as_str().to_string();
                        let data_type = parts.next().unwrap().as_str().to_uppercase();
                        columns.push((name, data_type));
                    }
                }
            }
            Rule::primary_key_clause => {
                for pk_pair in inner.into_inner() {
                    if pk_pair.as_rule() == Rule::identifier_list {
                        for id in pk_pair.into_inner() {
                            if id.as_rule() == Rule::identifier {
                                primary_key.push(id.as_str().to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Statement::CreateNodeTable {
        table_name,
        columns,
        primary_key,
    }
}

fn build_create_rel_table(pair: pest::iterators::Pair<Rule>) -> Statement {
    let mut table_name = String::new();
    let mut src_table = String::new();
    let mut dst_table = String::new();
    let mut columns = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                if table_name.is_empty() {
                    table_name = inner.as_str().to_string();
                }
            }
            Rule::from_to_clause => {
                let mut idents = inner.into_inner();
                src_table = idents.next().unwrap().as_str().to_string();
                dst_table = idents.next().unwrap().as_str().to_string();
            }
            Rule::rel_property_list => {
                for col_pair in inner.into_inner() {
                    if col_pair.as_rule() == Rule::column_def {
                        let mut parts = col_pair.into_inner();
                        let name = parts.next().unwrap().as_str().to_string();
                        let data_type = parts.next().unwrap().as_str().to_uppercase();
                        columns.push((name, data_type));
                    }
                }
            }
            _ => {}
        }
    }

    Statement::CreateRelTable {
        table_name,
        src_table,
        dst_table,
        columns,
    }
}

fn build_create_node(pair: pest::iterators::Pair<Rule>) -> Result<Statement> {
    let mut label = String::new();
    let mut properties = Vec::new();

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::node_pattern {
            for node_inner in inner.into_inner() {
                match node_inner.as_rule() {
                    Rule::identifier => {
                        label = node_inner.as_str().to_string();
                    }
                    Rule::properties => {
                        for prop_pair in node_inner.into_inner() {
                            if prop_pair.as_rule() == Rule::property_list {
                                for prop in prop_pair.into_inner() {
                                    if prop.as_rule() == Rule::property {
                                        let mut parts = prop.into_inner();
                                        let name = parts.next().unwrap().as_str().to_string();
                                        let lit_pair = parts.next().unwrap();
                                        let literal = build_literal(lit_pair)?;
                                        properties.push((name, literal));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(Statement::CreateNode { label, properties })
}

fn build_match_create(pair: pest::iterators::Pair<Rule>) -> Result<Statement> {
    let mut src_node = None;
    let mut dst_node = None;
    let mut rel_type = String::new();
    let mut rel_props = Vec::new();
    let mut src_var = String::new();
    let mut dst_var = String::new();

    let mut match_filters: Vec<NodeFilter> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::match_node_filter => {
                let filter = build_node_filter(inner)?;
                match_filters.push(filter);
            }
            Rule::relationship_pattern => {
                for rel_inner in inner.into_inner() {
                    match rel_inner.as_rule() {
                        Rule::identifier => {
                            if src_var.is_empty() {
                                src_var = rel_inner.as_str().to_string();
                            } else {
                                dst_var = rel_inner.as_str().to_string();
                            }
                        }
                        Rule::rel_type_pattern => {
                            for type_inner in rel_inner.into_inner() {
                                match type_inner.as_rule() {
                                    Rule::identifier => {
                                        rel_type = type_inner.as_str().to_string();
                                    }
                                    Rule::rel_properties => {
                                        for prop_pair in type_inner.into_inner() {
                                            if prop_pair.as_rule() == Rule::property_list {
                                                for prop in prop_pair.into_inner() {
                                                    if prop.as_rule() == Rule::property {
                                                        let mut parts = prop.into_inner();
                                                        let name = parts
                                                            .next()
                                                            .unwrap()
                                                            .as_str()
                                                            .to_string();
                                                        let lit_pair = parts.next().unwrap();
                                                        let literal = build_literal(lit_pair)?;
                                                        rel_props.push((name, literal));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if match_filters.len() >= 2 {
        src_node = Some(match_filters.remove(0));
        dst_node = Some(match_filters.remove(0));
    }

    Ok(Statement::MatchCreate {
        src_node: src_node.ok_or_else(|| RuzuError::ParseError {
            line: 0,
            col: 0,
            message: "Missing source node in MATCH CREATE".into(),
        })?,
        dst_node: dst_node.ok_or_else(|| RuzuError::ParseError {
            line: 0,
            col: 0,
            message: "Missing destination node in MATCH CREATE".into(),
        })?,
        rel_type,
        rel_props,
        src_var,
        dst_var,
    })
}

fn build_node_filter(pair: pest::iterators::Pair<Rule>) -> Result<NodeFilter> {
    let mut var = String::new();
    let mut label = String::new();
    let mut property_filter = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                if var.is_empty() {
                    var = inner.as_str().to_string();
                } else {
                    label = inner.as_str().to_string();
                }
            }
            Rule::property_filter => {
                for filter_inner in inner.into_inner() {
                    if filter_inner.as_rule() == Rule::property_key_value {
                        let mut parts = filter_inner.into_inner();
                        let key = parts.next().unwrap().as_str().to_string();
                        let lit_pair = parts.next().unwrap();
                        let value = build_literal(lit_pair)?;
                        property_filter = Some((key, value));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(NodeFilter {
        var,
        label,
        property_filter,
    })
}

/// Extracts an integer literal from a clause pair (used for SKIP and LIMIT).
fn parse_integer_clause(pair: pest::iterators::Pair<Rule>, name: &str) -> Result<i64> {
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::integer_literal {
            return inner.as_str().parse().map_err(|_| RuzuError::ParseError {
                line: 0,
                col: 0,
                message: format!("Invalid {name} value"),
            });
        }
    }
    Err(RuzuError::ParseError {
        line: 0,
        col: 0,
        message: format!("Missing integer in {name} clause"),
    })
}

/// Parses a `match_rel_pattern` pair into its component parts.
fn build_rel_pattern(
    pair: pest::iterators::Pair<Rule>,
) -> Result<(Option<NodeFilter>, Option<NodeFilter>, Option<String>, String, Option<(u32, u32)>)> {
    let mut src_node = None;
    let mut dst_node = None;
    let mut rel_var = None;
    let mut rel_type = String::new();
    let mut path_bounds = None;

    for rel_inner in pair.into_inner() {
        match rel_inner.as_rule() {
            Rule::match_node_with_filter => {
                let node_filter = build_node_filter_with_optional_props(rel_inner)?;
                if src_node.is_none() {
                    src_node = Some(node_filter);
                } else {
                    dst_node = Some(node_filter);
                }
            }
            Rule::match_rel_type => {
                for type_inner in rel_inner.into_inner() {
                    match type_inner.as_rule() {
                        Rule::identifier => {
                            if rel_type.is_empty() {
                                rel_type = type_inner.as_str().to_string();
                            } else {
                                rel_var = Some(std::mem::take(&mut rel_type));
                                rel_type = type_inner.as_str().to_string();
                            }
                        }
                        Rule::path_length => {
                            path_bounds = Some(build_path_length(type_inner)?);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok((src_node, dst_node, rel_var, rel_type, path_bounds))
}

fn build_match_query(pair: pest::iterators::Pair<Rule>) -> Result<Statement> {
    let mut var = String::new();
    let mut label = String::new();
    let mut filter = None;
    let mut projections = Vec::new();
    let mut order_by = None;
    let mut skip = None;
    let mut limit = None;

    // Check if this is a relationship match or a simple node match
    let mut is_rel_match = false;
    let mut src_node = None;
    let mut dst_node = None;
    let mut rel_var = None;
    let mut rel_type = String::new();
    let mut path_bounds = None;

    for inner in pair.clone().into_inner() {
        if inner.as_rule() == Rule::match_rel_pattern {
            is_rel_match = true;
            break;
        }
    }

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::match_pattern => {
                let mut idents = inner.into_inner();
                var = idents.next().unwrap().as_str().to_string();
                label = idents.next().unwrap().as_str().to_string();
            }
            Rule::match_rel_pattern => {
                let result = build_rel_pattern(inner)?;
                src_node = result.0;
                dst_node = result.1;
                rel_var = result.2;
                rel_type = result.3;
                path_bounds = result.4;
            }
            Rule::where_clause => {
                for where_inner in inner.into_inner() {
                    if where_inner.as_rule() == Rule::expression {
                        filter = Some(build_expression(where_inner)?);
                    }
                }
            }
            Rule::return_clause => {
                for return_inner in inner.into_inner() {
                    if return_inner.as_rule() == Rule::return_item_list {
                        projections = build_return_item_list(return_inner)?;
                    }
                }
            }
            Rule::order_by_clause => {
                order_by = Some(build_order_by_clause(inner));
            }
            Rule::skip_clause => {
                skip = Some(parse_integer_clause(inner, "SKIP")?);
            }
            Rule::limit_clause => {
                limit = Some(parse_integer_clause(inner, "LIMIT")?);
            }
            _ => {}
        }
    }

    if is_rel_match {
        Ok(Statement::MatchRel {
            src_node: src_node.ok_or_else(|| RuzuError::ParseError {
                line: 0,
                col: 0,
                message: "Missing source node in relationship match".into(),
            })?,
            rel_var,
            rel_type,
            dst_node: dst_node.ok_or_else(|| RuzuError::ParseError {
                line: 0,
                col: 0,
                message: "Missing destination node in relationship match".into(),
            })?,
            filter,
            projections,
            order_by,
            skip,
            limit,
            path_bounds,
        })
    } else {
        Ok(Statement::Match {
            var,
            label,
            filter,
            projections,
            order_by,
            skip,
            limit,
        })
    }
}

fn build_return_item_list(pair: pest::iterators::Pair<Rule>) -> Result<Vec<ReturnItem>> {
    let mut items = Vec::new();

    for return_item in pair.into_inner() {
        if return_item.as_rule() == Rule::return_item {
            for inner in return_item.into_inner() {
                match inner.as_rule() {
                    Rule::aggregate_expr => {
                        items.push(build_aggregate_expr(inner)?);
                    }
                    Rule::projection => {
                        let mut parts = inner.into_inner();
                        let var = parts.next().unwrap().as_str().to_string();
                        let prop = parts.next().unwrap().as_str().to_string();
                        items.push(ReturnItem::projection(var, prop));
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(items)
}

fn build_aggregate_expr(pair: pest::iterators::Pair<Rule>) -> Result<ReturnItem> {
    let mut func: Option<AstAggregateFunction> = None;
    let mut input: Option<(String, String)> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::count_star => {
                // COUNT(*)
                return Ok(ReturnItem::aggregate(AstAggregateFunction::Count, None));
            }
            Rule::aggregate_function => {
                let func_name = inner.as_str();
                func = Some(AstAggregateFunction::parse(func_name).ok_or_else(|| RuzuError::ParseError {
                    line: 0,
                    col: 0,
                    message: format!("Unknown aggregate function: {func_name}"),
                })?);
            }
            Rule::projection => {
                let mut parts = inner.into_inner();
                let var = parts.next().unwrap().as_str().to_string();
                let prop = parts.next().unwrap().as_str().to_string();
                input = Some((var, prop));
            }
            _ => {}
        }
    }

    match func {
        Some(f) => Ok(ReturnItem::aggregate(f, input)),
        None => Err(RuzuError::ParseError {
            line: 0,
            col: 0,
            message: "Invalid aggregate expression: missing function".into(),
        }),
    }
}

fn build_order_by_clause(pair: pest::iterators::Pair<Rule>) -> Vec<OrderByItem> {
    let mut items = Vec::new();

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::order_by_item_list {
            for order_item in inner.into_inner() {
                if order_item.as_rule() == Rule::order_by_item {
                    let mut var = String::new();
                    let mut property = String::new();
                    let mut ascending = true; // Default to ASC

                    for item_inner in order_item.into_inner() {
                        match item_inner.as_rule() {
                            Rule::projection => {
                                let mut parts = item_inner.into_inner();
                                var = parts.next().unwrap().as_str().to_string();
                                property = parts.next().unwrap().as_str().to_string();
                            }
                            Rule::order_direction => {
                                ascending = item_inner.as_str().to_uppercase() == "ASC";
                            }
                            _ => {}
                        }
                    }

                    items.push(OrderByItem { var, property, ascending });
                }
            }
        }
    }

    items
}

fn build_path_length(pair: pest::iterators::Pair<Rule>) -> Result<(u32, u32)> {
    let mut min_hops = 0u32;
    let mut max_hops = 0u32;
    let mut first = true;

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::integer_literal {
            let value: u32 = inner.as_str().parse().map_err(|_| RuzuError::ParseError {
                line: 0,
                col: 0,
                message: "Invalid path length value".into(),
            })?;
            if first {
                min_hops = value;
                first = false;
            } else {
                max_hops = value;
            }
        }
    }

    if min_hops > max_hops {
        return Err(RuzuError::ParseError {
            line: 0,
            col: 0,
            message: format!("Invalid path length: min {min_hops} > max {max_hops}"),
        });
    }

    Ok((min_hops, max_hops))
}

fn build_node_filter_with_optional_props(pair: pest::iterators::Pair<Rule>) -> Result<NodeFilter> {
    let mut var = String::new();
    let mut label = String::new();
    let mut property_filter = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                if var.is_empty() {
                    var = inner.as_str().to_string();
                } else {
                    label = inner.as_str().to_string();
                }
            }
            Rule::property_filter => {
                for filter_inner in inner.into_inner() {
                    if filter_inner.as_rule() == Rule::property_key_value {
                        let mut parts = filter_inner.into_inner();
                        let key = parts.next().unwrap().as_str().to_string();
                        let lit_pair = parts.next().unwrap();
                        let value = build_literal(lit_pair)?;
                        property_filter = Some((key, value));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(NodeFilter {
        var,
        label,
        property_filter,
    })
}

fn build_expression(pair: pest::iterators::Pair<Rule>) -> Result<Expression> {
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::comparison {
            let mut parts = inner.into_inner();

            let projection = parts.next().unwrap();
            let mut proj_parts = projection.into_inner();
            let var = proj_parts.next().unwrap().as_str().to_string();
            let property = proj_parts.next().unwrap().as_str().to_string();

            let op_str = parts.next().unwrap().as_str();
            let op = ComparisonOp::parse(op_str).ok_or_else(|| RuzuError::ParseError {
                line: 0,
                col: 0,
                message: format!("Unknown operator: {op_str}"),
            })?;

            let lit_pair = parts.next().unwrap();
            let value = build_literal(lit_pair)?;

            return Ok(Expression {
                var,
                property,
                op,
                value,
            });
        }
    }

    Err(RuzuError::ParseError {
        line: 0,
        col: 0,
        message: "Invalid expression".into(),
    })
}

fn build_literal(pair: pest::iterators::Pair<Rule>) -> Result<Literal> {
    let rule = pair.as_rule();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::string_literal => {
                let s = inner.as_str();
                let content = &s[1..s.len() - 1];
                return Ok(Literal::String(content.to_string()));
            }
            Rule::integer_literal => {
                let n: i64 = inner.as_str().parse().map_err(|_| RuzuError::ParseError {
                    line: 0,
                    col: 0,
                    message: format!("Invalid integer: {}", inner.as_str()),
                })?;
                return Ok(Literal::Int64(n));
            }
            Rule::float_literal => {
                let f: f64 = inner.as_str().parse().map_err(|_| RuzuError::ParseError {
                    line: 0,
                    col: 0,
                    message: format!("Invalid float: {}", inner.as_str()),
                })?;
                if !f.is_finite() {
                    return Err(RuzuError::ParseError {
                        line: 0,
                        col: 0,
                        message: format!("Invalid FLOAT64 value: {} (NaN and Infinity are not allowed)", inner.as_str()),
                    });
                }
                return Ok(Literal::Float64(f));
            }
            Rule::bool_literal => {
                let b = inner.as_str().eq_ignore_ascii_case("true");
                return Ok(Literal::Bool(b));
            }
            _ => {}
        }
    }

    match rule {
        Rule::string_literal => Err(RuzuError::ParseError {
            line: 0,
            col: 0,
            message: "Empty string literal".into(),
        }),
        Rule::integer_literal => Err(RuzuError::ParseError {
            line: 0,
            col: 0,
            message: "Empty integer literal".into(),
        }),
        _ => Err(RuzuError::ParseError {
            line: 0,
            col: 0,
            message: "Invalid literal".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_copy_basic() {
        let stmt = parse_query("COPY Person FROM 'test.csv'").unwrap();
        match stmt {
            Statement::Copy {
                table_name,
                file_path,
                ..
            } => {
                assert_eq!(table_name, "Person");
                assert_eq!(file_path, "test.csv");
            }
            _ => panic!("Expected Copy statement"),
        }
    }

    #[test]
    fn test_parse_copy_with_delimiter() {
        let stmt = parse_query("COPY Person FROM 'test.csv' (DELIM = ';')").unwrap();
        match stmt {
            Statement::Copy {
                table_name,
                file_path,
                options,
            } => {
                assert_eq!(table_name, "Person");
                assert_eq!(file_path, "test.csv");
                assert_eq!(options.delimiter, Some(';'));
            }
            _ => panic!("Expected Copy statement"),
        }
    }

    #[test]
    fn test_parse_copy_with_ignore_errors() {
        let stmt = parse_query("COPY Person FROM 'test.csv' (IGNORE_ERRORS = true)").unwrap();
        match stmt {
            Statement::Copy { options, .. } => {
                assert_eq!(options.ignore_errors, Some(true));
            }
            _ => panic!("Expected Copy statement"),
        }
    }

    #[test]
    fn test_parse_copy_with_long_path() {
        // Test with a path similar to Windows tempfile paths
        let stmt = parse_query(
            "COPY Person FROM 'C:/Users/test/AppData/Local/Temp/abc123/test.csv' (DELIMITER = ';')",
        )
        .unwrap();
        match stmt {
            Statement::Copy {
                table_name,
                file_path,
                options,
            } => {
                assert_eq!(table_name, "Person");
                assert_eq!(
                    file_path,
                    "C:/Users/test/AppData/Local/Temp/abc123/test.csv"
                );
                assert_eq!(options.delimiter, Some(';'));
            }
            _ => panic!("Expected Copy statement"),
        }
    }

    #[test]
    fn test_parse_copy_with_delimiter_long_name() {
        let stmt = parse_query("COPY Person FROM 'test.csv' (DELIMITER = ';')").unwrap();
        match stmt {
            Statement::Copy { options, .. } => {
                assert_eq!(options.delimiter, Some(';'));
            }
            _ => panic!("Expected Copy statement"),
        }
    }
}

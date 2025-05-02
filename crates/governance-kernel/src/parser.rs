use pest::Parser;
use pest::iterators::{Pair, Pairs};
use pest::error::Error;
use pest_derive::Parser;

use super::ast::*;

#[derive(Parser)]
#[grammar = "ccl.pest"]
pub struct CclParser;

/// Parse CCL content into an AST
pub fn parse_ccl(ccl_content: &str) -> Result<CclRoot, Error<Rule>> {
    // Parse the input content with our grammar
    let pairs = CclParser::parse(Rule::ccl_document, ccl_content)?;
    
    // Get the top-level pair (ccl_document)
    let document_pair = pairs.into_iter().next().unwrap();
    
    // Process the template_declaration, which should be the only child of ccl_document
    let mut inner_pairs = document_pair.into_inner();
    let template_declaration = inner_pairs.next().unwrap();
    
    // Extract template_type and content object from template_declaration
    let mut decl_pairs = template_declaration.into_inner();
    let template_type = decl_pairs.next().unwrap().as_str().to_string();
    let content_object = parse_value(decl_pairs.next().unwrap())?;
    
    // Construct and return the CclRoot
    Ok(CclRoot {
        template_type,
        content: content_object,
    })
}

/// Parse a CCL value
fn parse_value(pair: Pair<Rule>) -> Result<CclValue, Error<Rule>> {
    match pair.as_rule() {
        Rule::object => parse_object(pair),
        Rule::array => parse_array(pair),
        Rule::string_literal => {
            // Remove quotes from string literal
            let inner = pair.into_inner().next().unwrap().as_str();
            Ok(CclValue::String(inner.to_string()))
        },
        Rule::number_literal => {
            // Parse the number
            let num_str = pair.as_str();
            match num_str.parse::<f64>() {
                Ok(num) => Ok(CclValue::Number(num)),
                Err(_) => Err(Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: format!("Failed to parse number: {}", num_str)
                    },
                    pair.as_span(),
                ))
            }
        },
        Rule::boolean_literal => {
            // Parse the boolean
            let bool_str = pair.as_str();
            Ok(CclValue::Boolean(bool_str == "true"))
        },
        Rule::null_literal => {
            // Return null value
            Ok(CclValue::Null)
        },
        Rule::identifier => {
            // Return identifier as is
            Ok(CclValue::Identifier(pair.as_str().to_string()))
        },
        _ => {
            // Handle value rule directly
            if pair.as_rule() == Rule::value {
                if let Some(inner) = pair.into_inner().next() {
                    return parse_value(inner);
                }
            }
            
            // Unexpected rule
            Err(Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Unexpected rule: {:?}", pair.as_rule())
                },
                pair.as_span(),
            ))
        }
    }
}

/// Parse a CCL object
fn parse_object(pair: Pair<Rule>) -> Result<CclValue, Error<Rule>> {
    let mut pairs = Vec::new();
    
    // Iterate through object pairs
    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::pair {
            let mut pair_parts = inner_pair.into_inner();
            
            // First part is key (string or identifier)
            let key_pair = pair_parts.next().unwrap();
            let key = match key_pair.as_rule() {
                Rule::string_literal => {
                    // String literal without quotes
                    key_pair.into_inner().next().unwrap().as_str().to_string()
                },
                Rule::identifier => {
                    // Identifier as is
                    key_pair.as_str().to_string()
                },
                _ => {
                    return Err(Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: format!("Expected string or identifier as key, got: {:?}", key_pair.as_rule())
                        },
                        key_pair.as_span(),
                    ))
                }
            };
            
            // Second part is value
            let value_pair = pair_parts.next().unwrap();
            let value = parse_value(value_pair)?;
            
            // Add pair to object
            pairs.push(CclPair { key, value });
        }
    }
    
    Ok(CclValue::Object(pairs))
}

/// Parse a CCL array
fn parse_array(pair: Pair<Rule>) -> Result<CclValue, Error<Rule>> {
    let mut values = Vec::new();
    
    // Iterate through array values
    for inner_pair in pair.into_inner() {
        if inner_pair.as_rule() == Rule::value {
            let value = parse_value(inner_pair)?;
            values.push(value);
        }
    }
    
    Ok(CclValue::Array(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_template() {
        let input = r#"coop_bylaws {
            "name": "Test Cooperative",
            "description": "A test cooperative for CCL parsing",
            "founding_date": "2023-01-01"
        }"#;
        
        let result = parse_ccl(input).expect("Failed to parse simple template");
        
        assert_eq!(result.template_type, "coop_bylaws");
        if let CclValue::Object(pairs) = &result.content {
            assert_eq!(pairs.len(), 3);
            
            assert_eq!(pairs[0].key, "name");
            assert_eq!(pairs[0].value, CclValue::String("Test Cooperative".to_string()));
            
            assert_eq!(pairs[1].key, "description");
            assert_eq!(pairs[1].value, CclValue::String("A test cooperative for CCL parsing".to_string()));
            
            assert_eq!(pairs[2].key, "founding_date");
            assert_eq!(pairs[2].value, CclValue::String("2023-01-01".to_string()));
        } else {
            panic!("Expected Object, got {:?}", result.content);
        }
    }
    
    #[test]
    fn test_parse_nested_objects() {
        let input = r#"community_charter {
            "name": "Test Community",
            "governance": {
                "decision_making": "consent",
                "quorum": 0.75,
                "majority": 0.66
            }
        }"#;
        
        let result = parse_ccl(input).expect("Failed to parse nested objects");
        
        assert_eq!(result.template_type, "community_charter");
        if let CclValue::Object(pairs) = &result.content {
            assert_eq!(pairs.len(), 2);
            
            assert_eq!(pairs[0].key, "name");
            assert_eq!(pairs[0].value, CclValue::String("Test Community".to_string()));
            
            assert_eq!(pairs[1].key, "governance");
            if let CclValue::Object(gov_pairs) = &pairs[1].value {
                assert_eq!(gov_pairs.len(), 3);
                assert_eq!(gov_pairs[0].key, "decision_making");
                assert_eq!(gov_pairs[0].value, CclValue::String("consent".to_string()));
                
                assert_eq!(gov_pairs[1].key, "quorum");
                assert_eq!(gov_pairs[1].value, CclValue::Number(0.75));
                
                assert_eq!(gov_pairs[2].key, "majority");
                assert_eq!(gov_pairs[2].value, CclValue::Number(0.66));
            } else {
                panic!("Expected Object for governance, got {:?}", pairs[1].value);
            }
        } else {
            panic!("Expected Object, got {:?}", result.content);
        }
    }
    
    #[test]
    fn test_parse_arrays() {
        let input = r#"resolution {
            "title": "Test Resolution",
            "supporters": ["Alice", "Bob", "Charlie"],
            "votes": [true, false, true, true]
        }"#;
        
        let result = parse_ccl(input).expect("Failed to parse arrays");
        
        if let CclValue::Object(pairs) = &result.content {
            assert_eq!(pairs[1].key, "supporters");
            if let CclValue::Array(supporters) = &pairs[1].value {
                assert_eq!(supporters.len(), 3);
                assert_eq!(supporters[0], CclValue::String("Alice".to_string()));
                assert_eq!(supporters[1], CclValue::String("Bob".to_string()));
                assert_eq!(supporters[2], CclValue::String("Charlie".to_string()));
            } else {
                panic!("Expected Array for supporters");
            }
            
            assert_eq!(pairs[2].key, "votes");
            if let CclValue::Array(votes) = &pairs[2].value {
                assert_eq!(votes.len(), 4);
                assert_eq!(votes[0], CclValue::Boolean(true));
                assert_eq!(votes[1], CclValue::Boolean(false));
                assert_eq!(votes[2], CclValue::Boolean(true));
                assert_eq!(votes[3], CclValue::Boolean(true));
            } else {
                panic!("Expected Array for votes");
            }
        }
    }
    
    #[test]
    fn test_parse_comments() {
        let input = r#"budget_proposal {
            // This is a single line comment
            "amount": 1000.50, /* This is a 
            multi-line comment */
            "purpose": "Test budget proposal"
        }"#;
        
        let result = parse_ccl(input).expect("Failed to parse with comments");
        
        assert_eq!(result.template_type, "budget_proposal");
        if let CclValue::Object(pairs) = &result.content {
            assert_eq!(pairs.len(), 2);
            assert_eq!(pairs[0].key, "amount");
            assert_eq!(pairs[0].value, CclValue::Number(1000.50));
        }
    }
    
    #[test]
    fn test_parse_error() {
        let input = r#"invalid_template {
            "missing_comma": "value"
            "another_key": "value"
        }"#;
        
        assert!(parse_ccl(input).is_err(), "Expected parsing error");
    }
} 
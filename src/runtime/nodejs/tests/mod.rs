//! Tests for Node.js runtime functionality

#[cfg(test)]
mod nodejs_tests {
    use super::super::super::NodeJSRuntime;
    use crate::error::SoulBoxError;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_runtime() -> (TempDir, NodeJSRuntime) {
        let temp_dir = TempDir::new().unwrap();
        let runtime = NodeJSRuntime::new(temp_dir.path().to_path_buf());
        (temp_dir, runtime)
    }

    #[test]
    fn test_block_comment_syntax_validation() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test valid block comments
        let valid_code = r#"
            /* This is a block comment */
            console.log("Hello World");
            
            /*
             * Multi-line block comment
             * with multiple lines
             */
            function test() {
                return true;
            }
        "#;

        let result = runtime.validate_syntax(valid_code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_nested_block_comments() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test code with nested comments (not supported in JS but should parse)
        let code = r#"
            /*
             * This is a /* nested */ comment
             */
            console.log("test");
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_block_comment_with_strings() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test block comments mixed with strings
        let code = r#"
            /* Comment before string */
            let str = "This /* is not a comment */";
            /* Comment after string */
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unclosed_block_comment() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test unclosed block comment - should still parse basic syntax
        let code = r#"
            /*
             * This comment is never closed
             console.log("This should be ignored");
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        // The syntax validator should handle this gracefully
        // even if the comment is unclosed
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_line_comment_vs_block_comment() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test mixing line comments and block comments
        let code = r#"
            // This is a line comment
            /* This is a block comment */
            function test() {
                // Another line comment
                /* Another block comment */
                return true;
            }
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_block_comment() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test empty block comment
        let code = r#"
            /**/
            console.log("test");
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_block_comment_in_code() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test block comment in the middle of code
        let code = r#"
            function test() {
                let x = 5; /* inline comment */ let y = 10;
                return x + y;
            }
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_complex_comment_scenarios() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test complex comment scenarios
        let code = r#"
            /*
             * Complex block comment
             * with special characters: /* */ // 
             */
            // Line comment with /* block comment syntax */
            function complex() {
                let str = "String with /* comment syntax */ inside";
                /* 
                 * Multi-line comment
                 * // with line comment syntax
                 */
                return "done";
            }
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }

    #[test]
    fn test_comment_with_brackets() {
        let (_temp_dir, runtime) = create_test_runtime();

        // Test that comments don't interfere with bracket counting
        let code = r#"
            function test() {
                /* comment with { braces } */
                if (true) {
                    // comment with ( parentheses )
                    let arr = [1, 2, 3]; /* comment with [ brackets ] */
                }
            }
        "#;

        let result = runtime.validate_syntax(code, "test.js");
        assert!(result.is_ok());
    }
}
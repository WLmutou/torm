//! SQL 注入防护工具模块
//!
//! 提供一组轻量、无外部依赖的工具，用于降低 SQL 注入风险：
//!
//! - **标识符校验/转义**：校验并引用表名、列名、别名等数据库标识符，
//!   防止通过标识符注入任意 SQL。
//! - **字符串字面量转义**：对单引号等特殊字符进行转义，保证字符串值安全。
//! - **危险模式检测**：检测常见 SQL 注入攻击模式，帮助发现可疑 SQL。
//!
//! # 使用建议
//!
//! - 凡是会被拼接到 SQL 中的**标识符**（表名、列名、别名、ORDER BY 列等），
//!   都应使用 [`SqlSanitizer::identifier`] 或 [`validate_identifier`] 进行校验与引用。
//! - 凡是作为**值**传入的字符串，尽量使用参数化绑定（`?` 占位符），
//!   如需字面量则使用 [`escape_string`] 进行转义。
//! - 对来自外部的原始 SQL（如 `where_raw`、`on_condition`），
//!   可使用 [`contains_injection_pattern`] 做前置审计。
//!
//! # 示例
//!
//! ```
//! use torm::utils::sql_safety::{SqlSanitizer, validate_identifier, escape_string};
//!
//! // 校验并安全引用标识符
//! assert_eq!(validate_identifier("user_name"), Ok("user_name".to_string()));
//! assert!(validate_identifier("user_name; DROP TABLE users").is_err());
//! assert_eq!(SqlSanitizer::identifier("user_name"), "user_name");
//! assert_eq!(SqlSanitizer::identifier("select"), "`select`");
//!
//! // 字符串转义
//! assert_eq!(escape_string("O'Reilly"), "O''Reilly");
//! ```

/// 常见 SQL 危险关键字（用于注入模式检测）。
const DANGEROUS_KEYWORDS: &[&str] = &[
    "select", "insert", "update", "delete", "drop", "alter", "truncate",
    "create", "grant", "revoke", "union", "into", "outfile", "load_file",
    "sleep", "benchmark", "exec", "execute", "xp_cmdshell", "shutdown",
    "attach", "pragma", "vacuum", "reindex",
];

/// 合法标识符中允许出现的字符。
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// 非法标识符的出错提示前缀。
const ERR_PREFIX: &str = "Invalid SQL identifier";

/// 判断给定的数据库标识符（表名/列名/别名）是否合法。
///
/// 只允许字母、数字、下划线、`$`，且不能以数字开头、不能为空。
/// 用于防止通过标识符注入任意 SQL。
pub fn validate_identifier(identifier: &str) -> Result<String, String> {
    let id = identifier.trim();
    if id.is_empty() {
        return Err(format!("{}: empty", ERR_PREFIX));
    }
    let first = id.chars().next().unwrap_or('\0');
    if first.is_ascii_digit() {
        return Err(format!("{}: cannot start with a digit: `{}`", ERR_PREFIX, id));
    }
    if !id.chars().all(is_ident_char) {
        return Err(format!("{}: contains invalid characters: `{}`", ERR_PREFIX, id));
    }
    Ok(id.to_string())
}

/// 校验并规范化一个可能是限定名（如 `table.column`、`COUNT(*)`）的标识符片段。
///
/// 该方法针对 ORM 中常见的 `users.id`、`COUNT(*)` 等表达式做了放宽处理：
/// - 允许出现 `.`（限定符分隔）
/// - 允许形如 `func(...)`、`*` 的聚合/通配写法
///
/// 仍会拒绝含单引号、分号、注释等危险字符的输入。
pub fn validate_qualified_identifier(identifier: &str) -> Result<String, String> {
    let id = identifier.trim();
    if id.is_empty() {
        return Err(format!("{}: empty", ERR_PREFIX));
    }
    // 允许基本字母/数字/下划线/$. 及括号、点、星号（用于 func(...) 与 *）
    let ok = id.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || c == '_'
            || c == '$'
            || c == '.'
            || c == '('
            || c == ')'
            || c == '*'
            || c == ' '
    });
    if !ok {
        return Err(format!(
            "{}: contains invalid characters: `{}`",
            ERR_PREFIX, id
        ));
    }
    // 仍拒绝明显注入特征
    if contains_injection_pattern(id).is_some() {
        return Err(format!("{}: contains injection pattern: `{}`", ERR_PREFIX, id));
    }
    Ok(id.to_string())
}

/// 判断是否为需要引用的保留关键字（不区分大小写）。
pub fn is_reserved_word(identifier: &str) -> bool {
    let upper = identifier.to_ascii_uppercase();
    RESERVED_WORDS.contains(&upper.as_str())
}

/// 常用 SQL 保留字（仅用于标识符自动引用）。
const RESERVED_WORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP",
    "ALTER", "TABLE", "INDEX", "AND", "OR", "NOT", "NULL", "IN", "LIKE",
    "BETWEEN", "ORDER", "GROUP", "BY", "HAVING", "LIMIT", "OFFSET", "JOIN",
    "INNER", "LEFT", "RIGHT", "FULL", "ON", "AS", "UNION", "ALL", "DISTINCT",
    "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "UNIQUE", "CHECK", "DEFAULT",
    "CASE", "WHEN", "THEN", "ELSE", "END", "IS", "DESC", "ASC", "COUNT",
    "SUM", "AVG", "MIN", "MAX", "VALUES", "SET", "TO", "EXISTS", "CASCADE",
];

/// 将标识符用安全引号包裹，返回可在 SQL 中安全使用的形式。
///
/// 若标识符非法，返回 `None`。合法的标识符会按需加上反引号
/// （当它是保留字时），从而避免在 SQL 拼接处被注入。
pub fn quote_identifier(identifier: &str) -> Option<String> {
    match validate_identifier(identifier) {
        Ok(id) => {
            if is_reserved_word(&id) {
                Some(format!("`{}`", id))
            } else {
                Some(id)
            }
        }
        Err(_) => None,
    }
}

/// 转义 SQL 字符串字面量中的特殊字符。
///
/// 目前处理单引号（`'` → `''`）。使用时应当用 `'` 包裹：
/// `format!("'{}'", escape_string(input))`。
pub fn escape_string(input: &str) -> String {
    input.replace('\'', "''")
}

/// 检测 SQL 字符串中是否包含常见的注入危险模式。
///
/// 返回命中第一个危险片段的位置（`Option<(usize, &str)>`），
/// 其中 `usize` 为命中片段的起始字节偏移，`&str` 为命中的关键字。
/// 若未命中任何危险模式，返回 `None`。
///
/// 注意：该检测是启发式的，用于辅助审计；不能替代参数化查询。
pub fn contains_injection_pattern(sql: &str) -> Option<(usize, &str)> {
    let lower = sql.to_ascii_lowercase();
    let bytes: Vec<char> = lower.chars().collect();
    let mut i = 0;
    // 记录是否已看到 SQL 的起始关键字（SELECT/INSERT/UPDATE/DELETE）。
    // 只有非起始位置的 DML 关键字才视为可疑（可能来自拼接的第二条语句或子查询）。
    let mut first_token_done = false;

    while i < bytes.len() {
        // 跳过空白
        if bytes[i].is_whitespace() {
            i += 1;
            continue;
        }
        // 跳过字符串字面量
        if bytes[i] == '\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == '\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == '\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                // 语句分隔符分号不应出现在 ORM 生成的单个字符串字面量内：
                // 遇到即视为字面量已结束，以便继续扫描其后可能的注入语句。
                if bytes[i] == ';' {
                    first_token_done = true;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // 跳过注释
        if bytes[i] == '-' && i + 1 < bytes.len() && bytes[i + 1] == '-' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }

        // 判断当前字符是否为词边界起点
        if !is_ident_char(bytes[i]) {
            // 出现语句分隔符（分号）意味着后面可能跟第二条语句
            if bytes[i] == ';' {
                first_token_done = true;
            }
            i += 1;
            continue;
        }

        // 读取一个词
        let start = i;
        while i < bytes.len() && is_ident_char(bytes[i]) {
            i += 1;
        }
        let word: String = bytes[start..i].iter().collect();

        // 匹配危险关键字
        let matched: Option<&str> = DANGEROUS_KEYWORDS.iter().copied().find(|kw| {
            kw.len() == word.len() && kw.eq_ignore_ascii_case(&word)
        });
        if let Some(kw) = matched {
            // DDL / 危险操作关键字，无论出现在哪里都视为可疑
            let always_suspicious = matches!(kw, "drop" | "alter" | "truncate" | "create" | "grant" | "revoke");
            // DML 关键字只有在语句中后段出现（first_token_done 之后）才可疑
            if always_suspicious || (first_token_done && kw_is_mid(kw)) {
                return Some((start, sql.get(start..i).unwrap_or("")));
            }
            // 命中但未判定为危险（例如作为起始的 SELECT），标记首词已出现
            first_token_done = true;
        } else {
            // 经典恒真式注入：`OR 1=1` / `AND 2=2`（常量与常量比较）在语句中后段出现即视为可疑
            if first_token_done
                && (word.eq_ignore_ascii_case("or") || word.eq_ignore_ascii_case("and"))
                && is_tautology_literal(&bytes, i)
            {
                return Some((start, sql.get(start..i).unwrap_or("")));
            }
            first_token_done = true;
        }
    }
    None
}

/// 判断 `OR`/`AND` 之后是否为 `数字 = 数字` 的恒真/恒假常量比较（如 `1=1`）。
fn is_tautology_literal(bytes: &[char], from: usize) -> bool {
    let mut j = from;
    // 跳过空白
    while j < bytes.len() && bytes[j].is_whitespace() {
        j += 1;
    }
    // 第一个数字
    let start = j;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j == start {
        return false;
    }
    // 跳过空白
    while j < bytes.len() && bytes[j].is_whitespace() {
        j += 1;
    }
    // 等号
    if j >= bytes.len() || bytes[j] != '=' {
        return false;
    }
    j += 1;
    // 跳过空白
    while j < bytes.len() && bytes[j].is_whitespace() {
        j += 1;
    }
    // 第二个数字
    let end_start = j;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    j > end_start
}

/// 判断关键字是否为"需要出现在语句中后段才算危险"的 DML 关键字。
fn kw_is_mid(kw: &str) -> bool {
    matches!(
        kw,
        "select" | "insert" | "update" | "delete" | "union" | "into" | "outfile" | "load_file"
    )
}

/// 一个轻量级的 SQL 安全工具，提供面向拼接场景的便捷 API。
#[derive(Debug, Clone, Copy, Default)]
pub struct SqlSanitizer;

impl SqlSanitizer {
    /// 校验并规范化标识符。
    ///
    /// - 合法：返回原始（或保留字被反引号包裹）的标识符，可直接拼接。
    /// - 非法：返回一个不含危险字符的空安全占位符 `""`（并打印警告）。
    ///
    /// 使用反引号是为了兼容 SQLite / MySQL；PostgreSQL 也可接受反引号之外的
    /// 双引号，但为简单统一，这里统一使用反引号。
    pub fn identifier(identifier: &str) -> String {
        quote_identifier(identifier).unwrap_or_else(|| {
            eprintln!(
                "[torm::sql_safety] rejected unsafe identifier: {:?}",
                identifier
            );
            String::new()
        })
    }

    /// 转义字符串字面量中的单引号，适合用于字面量值。
    pub fn escape(unescaped: &str) -> String {
        escape_string(unescaped)
    }

    /// 校验并引用一个标识符，返回 `Option`。
    pub fn quote(identifier: &str) -> Option<String> {
        quote_identifier(identifier)
    }

    /// 检查一段 SQL 是否含有疑似注入的危险模式。
    pub fn check(sql: &str) -> Option<(usize, &str)> {
        contains_injection_pattern(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_identifier_ok() {
        assert_eq!(validate_identifier("user_name"), Ok("user_name".to_string()));
        assert_eq!(validate_identifier("_tmp"), Ok("_tmp".to_string()));
        assert_eq!(validate_identifier("table2"), Ok("table2".to_string()));
    }

    #[test]
    fn test_validate_identifier_rejects_injection() {
        assert!(validate_identifier("name; DROP TABLE users").is_err());
        assert!(validate_identifier("1abc").is_err());
        assert!(validate_identifier("col' OR '1'='1").is_err());
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("  ").is_err());
        assert!(validate_identifier("a b").is_err());
    }

    #[test]
    fn test_quote_identifier() {
        assert_eq!(quote_identifier("user_name"), Some("user_name".to_string()));
        assert_eq!(quote_identifier("select"), Some("`select`".to_string()));
        assert_eq!(quote_identifier("order"), Some("`order`".to_string()));
        assert_eq!(quote_identifier("bad name"), None);
        assert_eq!(quote_identifier("id; DROP"), None);
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("O'Reilly"), "O''Reilly");
        assert_eq!(escape_string("plain"), "plain");
        assert_eq!(escape_string("a''b"), "a''''b");
        assert_eq!(escape_string(""), "");
    }

    #[test]
    fn test_sanitizer_identifier() {
        assert_eq!(SqlSanitizer::identifier("user_name"), "user_name");
        assert_eq!(SqlSanitizer::identifier("select"), "`select`");
        assert_eq!(SqlSanitizer::identifier("bad name"), "");
        assert_eq!(SqlSanitizer::identifier("id; DROP TABLE users"), "");
    }

    #[test]
    fn test_contains_injection_pattern() {
        // 无注入
        assert!(contains_injection_pattern("SELECT * FROM users WHERE id = ?").is_none());

        // 在字符串字面量中的关键字不应误报
        assert!(contains_injection_pattern("SELECT * FROM users WHERE name = 'select'").is_none());
        assert!(contains_injection_pattern("SELECT * FROM users WHERE name = 'It''s a drop test'").is_none());

        // 明显注入
        assert!(contains_injection_pattern("'; DROP TABLE users; --").is_some());
        assert!(contains_injection_pattern("UNION SELECT * FROM admin").is_some());
        assert!(contains_injection_pattern("1 OR 1=1; DELETE FROM users").is_some());
    }

    #[test]
    fn test_contains_tautology_pattern() {
        // 经典恒真式注入 `OR 1=1`
        assert!(contains_injection_pattern("x = 'a' OR 1=1 --").is_some());
        assert!(contains_injection_pattern("password = 'x' OR 1=1").is_some());
        assert!(contains_injection_pattern("name = 'a' AND 2=2").is_some());

        // 正常 `OR 列 = 值` 不应误报
        assert!(contains_injection_pattern("SELECT * FROM users WHERE active = 1 OR deleted = 0").is_none());
        // 列名带数字比较也不应误报
        assert!(contains_injection_pattern("SELECT * FROM users WHERE age = 18").is_none());
    }
}

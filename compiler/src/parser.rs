use std::fmt;

use crate::ast::*;
use crate::lexer::{SpannedToken, Token};

pub fn parse(tokens: Vec<SpannedToken>) -> Result<Program, ParseError> {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    /// When true, suppress struct literal parsing (inside if/for/while conditions)
    no_struct_literal: bool,
}

impl Parser {
    fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser { tokens, pos: 0, no_struct_literal: false }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        self.skip_newlines();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }

        Ok(Program { statements })
    }

    // --- Helpers ---

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn current_span(&self) -> (usize, usize) {
        let t = &self.tokens[self.pos];
        (t.line, t.col)
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos].token;
        if !self.is_at_end() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        if self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            let (line, col) = self.current_span();
            Err(ParseError {
                message: format!("Expected {expected:?}, got {:?}", self.peek()),
                line,
                col,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            Token::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => {
                let (line, col) = self.current_span();
                Err(ParseError {
                    message: format!("Expected identifier, got {other:?}"),
                    line,
                    col,
                })
            }
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    fn at_statement_end(&self) -> bool {
        matches!(self.peek(), Token::Newline | Token::Eof | Token::RightBrace)
    }

    #[allow(dead_code)]
    fn expect_statement_end(&mut self) -> Result<(), ParseError> {
        if matches!(
            self.peek(),
            Token::Newline | Token::Eof | Token::RightBrace
        ) {
            if matches!(self.peek(), Token::Newline) {
                self.advance();
            }
            Ok(())
        } else {
            let (line, col) = self.current_span();
            Err(ParseError {
                message: format!("Expected end of statement, got {:?}", self.peek()),
                line,
                col,
            })
        }
    }

    // --- Statements ---

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        // Check for #[test] attribute
        let is_test = if matches!(self.peek(), Token::Hash) {
            let saved = self.pos;
            self.advance();
            if matches!(self.peek(), Token::LeftBracket) {
                self.advance();
                if let Token::Ident(attr) = self.peek().clone() {
                    if attr == "test" {
                        self.advance();
                        self.expect(&Token::RightBracket)?;
                        self.skip_newlines();
                        true
                    } else {
                        self.pos = saved;
                        false
                    }
                } else {
                    self.pos = saved;
                    false
                }
            } else {
                self.pos = saved;
                false
            }
        } else {
            false
        };

        // Check for @decorator
        let mut decorators: Vec<(String, Vec<Expr>)> = Vec::new();
        while matches!(self.peek(), Token::At) {
            self.advance();
            let name = self.expect_ident()?;
            let args = if matches!(self.peek(), Token::LeftParen) {
                self.advance();
                let mut dargs = Vec::new();
                while !matches!(self.peek(), Token::RightParen) {
                    if !dargs.is_empty() { self.expect(&Token::Comma)?; }
                    dargs.push(self.parse_expr()?);
                }
                self.expect(&Token::RightParen)?;
                dargs
            } else {
                Vec::new()
            };
            decorators.push((name, args));
            self.skip_newlines();
        }

        let is_pub = if matches!(self.peek(), Token::Pub) {
            self.advance();
            self.skip_newlines();
            true
        } else {
            false
        };

        match self.peek() {
            Token::Let => self.parse_let(),
            Token::Mut => self.parse_mut(),
            Token::Const => self.parse_const(),
            Token::Fn => self.parse_fn_decl(is_pub, is_test, decorators),
            Token::Struct => self.parse_struct_decl(is_pub),
            Token::Enum => self.parse_enum_decl(is_pub),
            Token::Trait => self.parse_trait_decl(is_pub),
            Token::Impl => self.parse_impl_block(),
            Token::Return => self.parse_return(),
            // defer expr → Statement::Expr with defer prefix (Go interop)
            Token::Ident(ref n) if n == "defer" => {
                self.advance();
                let expr = self.parse_expr()?;
                // Wrap in a special way the codegen can detect
                // We'll use Expr::Call with "defer" as callee
                Ok(Statement::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("defer".to_string())),
                    args: vec![crate::ast::CallArg { name: None, value: expr }],
                }))
            }
            Token::Spawn => {
                // sp expr → go expr (Go goroutine)
                self.advance();
                if matches!(self.peek(), Token::Fn) {
                    self.advance();
                    self.expect(&Token::LeftParen)?;
                    self.expect(&Token::RightParen)?;
                    let body = self.parse_block()?;
                    if matches!(self.peek(), Token::LeftParen) {
                        self.advance();
                        self.expect(&Token::RightParen)?;
                    }
                    let lambda = Expr::Lambda {
                        params: vec![],
                        body: Box::new(Expr::Block(body)),
                    };
                    // Call the lambda: func() { ... }()
                    let call = Expr::Call {
                        callee: Box::new(lambda),
                        args: vec![],
                    };
                    // Wrap in __spawn marker so codegen emits "go"
                    Ok(Statement::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("__spawn".to_string())),
                        args: vec![crate::ast::CallArg { name: None, value: call }],
                    }))
                } else {
                    let expr = self.parse_expr()?;
                    Ok(Statement::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("__spawn".to_string())),
                        args: vec![crate::ast::CallArg { name: None, value: expr }],
                    }))
                }
            }
            Token::Break => self.parse_break(),
            Token::Continue => {
                self.advance();
                Ok(Statement::Continue)
            }
            Token::Type => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect(&Token::Assign)?;
                let type_expr = self.parse_type()?;
                Ok(Statement::TypeAlias { name, type_expr })
            }
            Token::Use => self.parse_use(),
            Token::Module => self.parse_module(),
            _ => {
                let expr = self.parse_expr()?;

                // Check for assignment
                match self.peek() {
                    Token::Assign => {
                        self.advance();
                        let value = self.parse_expr()?;
                        Ok(Statement::Assignment {
                            target: expr,
                            op: AssignOp::Assign,
                            value,
                        })
                    }
                    Token::PlusAssign => {
                        self.advance();
                        let value = self.parse_expr()?;
                        Ok(Statement::Assignment {
                            target: expr,
                            op: AssignOp::AddAssign,
                            value,
                        })
                    }
                    Token::MinusAssign => {
                        self.advance();
                        let value = self.parse_expr()?;
                        Ok(Statement::Assignment {
                            target: expr,
                            op: AssignOp::SubAssign,
                            value,
                        })
                    }
                    Token::StarAssign => {
                        self.advance();
                        let value = self.parse_expr()?;
                        Ok(Statement::Assignment {
                            target: expr,
                            op: AssignOp::MulAssign,
                            value,
                        })
                    }
                    Token::SlashAssign => {
                        self.advance();
                        let value = self.parse_expr()?;
                        Ok(Statement::Assignment {
                            target: expr,
                            op: AssignOp::DivAssign,
                            value,
                        })
                    }
                    Token::PercentAssign => {
                        self.advance();
                        let value = self.parse_expr()?;
                        Ok(Statement::Assignment {
                            target: expr,
                            op: AssignOp::ModAssign,
                            value,
                        })
                    }
                    Token::ColonAssign => {
                        // x := expr → immutable binding (like let)
                        self.advance();
                        let value = self.parse_expr()?;
                        if let Expr::Ident(name) = expr {
                            Ok(Statement::Let {
                                name,
                                type_ann: None,
                                value,
                            })
                        } else if let Expr::Tuple(items) = expr {
                            // (a, b) := expr → destructuring let
                            let names: Vec<String> = items.into_iter().filter_map(|e| {
                                if let Expr::Ident(n) = e { Some(n) } else { None }
                            }).collect();
                            Ok(Statement::LetDestructure {
                                names,
                                value,
                                mutable: false,
                            })
                        } else {
                            let (line, col) = self.current_span();
                            Err(ParseError {
                                message: "Left side of := must be an identifier or tuple".to_string(),
                                line, col,
                            })
                        }
                    }
                    _ => Ok(Statement::Expr(expr)),
                }
            }
        }
    }

    fn parse_let(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // lt

        // Tuple destructuring: lt (a, b) = expr
        if matches!(self.peek(), Token::LeftParen) {
            return self.parse_destructure(false);
        }

        let name = self.expect_ident()?;
        let type_ann = if matches!(self.peek(), Token::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(Statement::Let {
            name,
            type_ann,
            value,
        })
    }

    fn parse_mut(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // mt

        // Tuple destructuring: mt (a, b) = expr
        if matches!(self.peek(), Token::LeftParen) {
            return self.parse_destructure(true);
        }

        let name = self.expect_ident()?;
        let type_ann = if matches!(self.peek(), Token::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(Statement::Mut {
            name,
            type_ann,
            value,
        })
    }

    fn parse_const(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // const
        let name = self.expect_ident()?;
        let type_ann = if matches!(self.peek(), Token::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(Statement::Const {
            name,
            type_ann,
            value,
        })
    }

    fn parse_destructure(&mut self, mutable: bool) -> Result<Statement, ParseError> {
        self.expect(&Token::LeftParen)?;
        let mut names = Vec::new();
        while !matches!(self.peek(), Token::RightParen) {
            if !names.is_empty() {
                self.expect(&Token::Comma)?;
            }
            names.push(self.expect_ident()?);
        }
        self.expect(&Token::RightParen)?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(Statement::LetDestructure {
            names,
            value,
            mutable,
        })
    }

    fn parse_fn_decl(&mut self, is_pub: bool, is_test: bool, decorators: Vec<(String, Vec<Expr>)>) -> Result<Statement, ParseError> {
        self.advance(); // fn
        let name = self.expect_ident()?;
        self.expect(&Token::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RightParen)?;

        let return_type = if matches!(self.peek(), Token::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();

        // Short-form: fn f(x) = expr
        let body = if matches!(self.peek(), Token::Assign) {
            self.advance();
            let expr = self.parse_expr()?;
            Block {
                statements: Vec::new(),
                expr: Some(Box::new(expr)),
            }
        } else {
            self.parse_block()?
        };

        Ok(Statement::FnDecl {
            name,
            params,
            return_type,
            body,
            is_pub,
            is_test,
            decorators,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !matches!(self.peek(), Token::RightParen) {
            if !params.is_empty() {
                self.expect(&Token::Comma)?;
                self.skip_newlines();
            }
            if matches!(self.peek(), Token::RightParen) {
                break;
            }

            // Handle `sf` parameter
            if matches!(self.peek(), Token::SelfValue) {
                self.advance();
                params.push(Param {
                    name: "sf".to_string(),
                    type_ann: None,
                    default: None,
                    is_variadic: false,
                });
                continue;
            }

            // Handle `mt sf`
            if matches!(self.peek(), Token::Mut) {
                let saved_pos = self.pos;
                self.advance();
                if matches!(self.peek(), Token::SelfValue) {
                    self.advance();
                    params.push(Param {
                        name: "sf".to_string(),
                        type_ann: None,
                        default: None,
                        is_variadic: false,
                    });
                    continue;
                }
                self.pos = saved_pos;
            }

            let is_variadic = if matches!(self.peek(), Token::Ellipsis) {
                self.advance();
                true
            } else {
                false
            };

            let name = self.expect_ident()?;

            let type_ann = if matches!(self.peek(), Token::Colon) {
                self.advance();
                Some(self.parse_type()?)
            } else {
                None
            };

            let default = if matches!(self.peek(), Token::Assign) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };

            params.push(Param {
                name,
                type_ann,
                default,
                is_variadic,
            });
        }
        Ok(params)
    }

    fn parse_struct_decl(&mut self, is_pub: bool) -> Result<Statement, ParseError> {
        self.advance(); // struct/class
        let name = self.expect_ident()?;
        let generics = self.parse_optional_generics()?;
        self.skip_newlines();

        // Handle empty struct: `struct Foo {}` or `struct Foo`
        if matches!(self.peek(), Token::LeftBrace) {
            self.advance(); // {
            self.skip_newlines();

            // Check if this is class-style (body starts with def/fn) or regular struct (fields)
            if matches!(self.peek(), Token::Fn | Token::Pub | Token::Hash | Token::At) {
                // Class-style: struct/class Name { methods... }
                // Parse as impl block methods, return StructDecl + ImplBlock
                // We return an ImplBlock here — the empty StructDecl is implicit
                let mut methods = Vec::new();
                while !matches!(self.peek(), Token::RightBrace | Token::Eof) {
                    let stmt = self.parse_statement()?;
                    methods.push(stmt);
                    self.skip_newlines();
                }
                self.expect(&Token::RightBrace)?;
                // Return as ImplBlock (the struct is implicitly declared)
                return Ok(Statement::ImplBlock {
                    trait_name: None,
                    target: name,
                    methods,
                });
            }

            // Regular struct with fields
            let mut fields = Vec::new();
            while !matches!(self.peek(), Token::RightBrace) {
                let field_pub = if matches!(self.peek(), Token::Pub) {
                    self.advance();
                    true
                } else {
                    false
                };
                let fname = self.expect_ident()?;
                self.expect(&Token::Colon)?;
                let ftype = self.parse_type()?;
                fields.push(Field {
                    name: fname,
                    type_ann: ftype,
                    is_pub: field_pub,
                });
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
                self.skip_newlines();
            }
            self.expect(&Token::RightBrace)?;

            return Ok(Statement::StructDecl {
                name,
                generics,
                fields,
                is_pub,
            });
        }

        // No brace — empty struct declaration (forward declaration)
        Ok(Statement::StructDecl {
            name,
            generics,
            fields: Vec::new(),
            is_pub,
        })
    }

    fn parse_enum_decl(&mut self, is_pub: bool) -> Result<Statement, ParseError> {
        self.advance(); // enum
        let name = self.expect_ident()?;
        let generics = self.parse_optional_generics()?;
        self.skip_newlines();
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut variants = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            let vname = self.expect_ident()?;
            let fields = if matches!(self.peek(), Token::LeftParen) {
                self.advance();
                let mut vfields = Vec::new();
                while !matches!(self.peek(), Token::RightParen) {
                    if !vfields.is_empty() {
                        self.expect(&Token::Comma)?;
                    }
                    // Try named field: name: Type
                    let saved = self.pos;
                    let maybe_name = if let Token::Ident(n) = self.peek().clone() {
                        self.advance();
                        if matches!(self.peek(), Token::Colon) {
                            self.advance();
                            Some(n)
                        } else {
                            self.pos = saved;
                            None
                        }
                    } else {
                        None
                    };
                    let type_ann = self.parse_type()?;
                    vfields.push(VariantField {
                        name: maybe_name,
                        type_ann,
                    });
                }
                self.expect(&Token::RightParen)?;
                vfields
            } else {
                Vec::new()
            };
            variants.push(Variant {
                name: vname,
                fields,
            });
            // Allow commas or newlines between variants
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(&Token::RightBrace)?;

        Ok(Statement::EnumDecl {
            name,
            generics,
            variants,
            is_pub,
        })
    }

    fn parse_trait_decl(&mut self, is_pub: bool) -> Result<Statement, ParseError> {
        self.advance(); // trait
        let name = self.expect_ident()?;
        let generics = self.parse_optional_generics()?;
        self.skip_newlines();
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            self.expect(&Token::Fn)?;
            let mname = self.expect_ident()?;
            self.expect(&Token::LeftParen)?;
            let params = self.parse_params()?;
            self.expect(&Token::RightParen)?;

            let return_type = if matches!(self.peek(), Token::Arrow) {
                self.advance();
                Some(self.parse_type()?)
            } else {
                None
            };

            self.skip_newlines();
            let body = if matches!(self.peek(), Token::LeftBrace) {
                Some(self.parse_block()?)
            } else {
                None
            };

            methods.push(TraitMethod {
                name: mname,
                params,
                return_type,
                body,
            });
            self.skip_newlines();
        }
        self.expect(&Token::RightBrace)?;

        Ok(Statement::TraitDecl {
            name,
            generics,
            methods,
            is_pub,
        })
    }

    fn parse_impl_block(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // impl
        let first_name = self.expect_ident()?;

        let (trait_name, target) = if matches!(self.peek(), Token::For) {
            self.advance();
            let target = self.expect_ident()?;
            (Some(first_name), target)
        } else {
            (None, first_name)
        };

        self.skip_newlines();
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            let is_pub = if matches!(self.peek(), Token::Pub) {
                self.advance();
                true
            } else {
                false
            };
            methods.push(self.parse_fn_decl(is_pub, false, Vec::new())?);
            self.skip_newlines();
        }
        self.expect(&Token::RightBrace)?;

        Ok(Statement::ImplBlock {
            trait_name,
            target,
            methods,
        })
    }

    fn parse_return(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // return
        if self.at_statement_end() {
            Ok(Statement::Return(None))
        } else {
            let expr = self.parse_expr()?;
            Ok(Statement::Return(Some(expr)))
        }
    }

    fn parse_break(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // break
        if self.at_statement_end() {
            Ok(Statement::Break(None))
        } else {
            let expr = self.parse_expr()?;
            Ok(Statement::Break(Some(expr)))
        }
    }

    fn parse_use(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // use
        let mut path = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            if matches!(self.peek(), Token::LeftBrace) {
                // use foo.bar.{A, B}
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek(), Token::RightBrace) {
                    if !items.is_empty() {
                        self.expect(&Token::Comma)?;
                        self.skip_newlines();
                    }
                    items.push(self.expect_ident()?);
                }
                self.expect(&Token::RightBrace)?;
                return Ok(Statement::Use {
                    path,
                    items: Some(items),
                    alias: None,
                });
            }
            path.push(self.expect_ident()?);
        }

        let alias = if matches!(self.peek(), Token::As) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        Ok(Statement::Use {
            path,
            items: None,
            alias,
        })
    }

    fn parse_module(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // module
        let mut path = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            path.push(self.expect_ident()?);
        }
        Ok(Statement::Module { path })
    }

    fn parse_optional_generics(&mut self) -> Result<Vec<String>, ParseError> {
        if !matches!(self.peek(), Token::Less) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut generics = Vec::new();
        while !matches!(self.peek(), Token::Greater) {
            if !generics.is_empty() {
                self.expect(&Token::Comma)?;
            }
            generics.push(self.expect_ident()?);
        }
        self.expect(&Token::Greater)?;
        Ok(generics)
    }

    // --- Types ---

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        // Fn(A, B) -> C  (Token::Fn or Ident("Fn"))
        if matches!(self.peek(), Token::Fn) || matches!(self.peek(), Token::Ident(ref n) if n == "Fn") {
            self.advance();
            self.expect(&Token::LeftParen)?;
            let mut params = Vec::new();
            while !matches!(self.peek(), Token::RightParen) {
                if !params.is_empty() {
                    self.expect(&Token::Comma)?;
                }
                params.push(self.parse_type()?);
            }
            self.expect(&Token::RightParen)?;
            self.expect(&Token::Arrow)?;
            let ret = self.parse_type()?;
            return Ok(TypeExpr::Function(params, Box::new(ret)));
        }

        // Tuple type: (A, B, C)
        if matches!(self.peek(), Token::LeftParen) {
            self.advance();
            let mut types = Vec::new();
            while !matches!(self.peek(), Token::RightParen) {
                if !types.is_empty() {
                    self.expect(&Token::Comma)?;
                }
                types.push(self.parse_type()?);
            }
            self.expect(&Token::RightParen)?;
            return Ok(TypeExpr::Tuple(types));
        }

        let name = self.expect_ident()?;
        if matches!(self.peek(), Token::Less) {
            self.advance();
            let mut args = Vec::new();
            while !matches!(self.peek(), Token::Greater | Token::ShiftRight) {
                if !args.is_empty() {
                    self.expect(&Token::Comma)?;
                }
                args.push(self.parse_type()?);
            }
            // Handle >> (shift right) as two > tokens for nested generics
            if matches!(self.peek(), Token::ShiftRight) {
                // Consume >> but logically only eat one >
                // Replace >> with > in the token stream
                let span = self.tokens[self.pos].clone();
                self.tokens[self.pos] = SpannedToken { token: Token::Greater, line: span.line, col: span.col + 1 };
            } else {
                self.advance(); // consume >
            }
            Ok(TypeExpr::Generic(name, args))
        } else if matches!(self.peek(), Token::LeftBracket) {
            // Python-style generics: List[Int], Map[String, Int], Tuple[Int, Int]
            self.advance();
            let mut args = Vec::new();
            while !matches!(self.peek(), Token::RightBracket) {
                if !args.is_empty() {
                    self.expect(&Token::Comma)?;
                }
                args.push(self.parse_type()?);
            }
            self.expect(&Token::RightBracket)?;
            Ok(TypeExpr::Generic(name, args))
        } else {
            Ok(TypeExpr::Named(name))
        }
    }

    // --- Blocks ---

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut statements = Vec::new();
        let mut final_expr = None;

        while !matches!(self.peek(), Token::RightBrace) {
            let stmt = self.parse_statement()?;
            self.skip_newlines();

            // If this is the last statement before `}` and it's an expression,
            // treat it as the block's return value
            if matches!(self.peek(), Token::RightBrace) {
                if let Statement::Expr(expr) = stmt {
                    final_expr = Some(Box::new(expr));
                    break;
                } else {
                    statements.push(stmt);
                }
            } else {
                statements.push(stmt);
            }
        }

        self.expect(&Token::RightBrace)?;

        Ok(Block {
            statements,
            expr: final_expr,
        })
    }

    // --- Expressions (Pratt parser) ---

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_pipe()
    }

    fn parse_pipe(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_nil_coalesce()?;
        while matches!(self.peek(), Token::PipeArrow) {
            self.advance();
            let right = self.parse_nil_coalesce()?;
            left = Expr::Pipe {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_nil_coalesce(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_or()?;
        while matches!(self.peek(), Token::NilCoalesce) {
            self.advance();
            let right = self.parse_or()?;
            left = Expr::NilCoalesce {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Token::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitwise_or()?;
        loop {
            let op = match self.peek() {
                Token::Eq => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                Token::Less => BinOp::Less,
                Token::Greater => BinOp::Greater,
                Token::LessEq => BinOp::LessEq,
                Token::GreaterEq => BinOp::GreaterEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_bitwise_or()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_bitwise_xor()?;
        // Note: Token::Pipe is also used for lambdas. Only treat as bitwise or
        // if it's clearly not a lambda context (i.e. we already have a left operand).
        // For now, bitwise | is lower priority; we skip it here to avoid ambiguity
        // with lambda syntax. Users should use explicit parentheses.
        while matches!(self.peek(), Token::Pipe) {
            // Lookahead: if next after | is an ident followed by |, it's a lambda
            // Otherwise treat as bitwise or. For now, skip bitwise or at this level.
            break;
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitwise_and()?;
        while matches!(self.peek(), Token::Caret) {
            self.advance();
            let right = self.parse_bitwise_and()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinOp::BitXor,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        while matches!(self.peek(), Token::Ampersand) {
            self.advance();
            let right = self.parse_shift()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinOp::BitAnd,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_range()?;
        loop {
            let op = match self.peek() {
                Token::ShiftLeft => BinOp::ShiftLeft,
                Token::ShiftRight => BinOp::ShiftRight,
                _ => break,
            };
            self.advance();
            let right = self.parse_range()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_addition()?;
        if matches!(self.peek(), Token::DotDot | Token::DotDotEq) {
            let inclusive = matches!(self.peek(), Token::DotDotEq);
            self.advance();
            let right = self.parse_addition()?;
            Ok(Expr::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive,
            })
        } else {
            Ok(left)
        }
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_power()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_unary()?;
        if matches!(self.peek(), Token::StarStar) {
            self.advance();
            let exp = self.parse_power()?; // right-associative
            Ok(Expr::BinaryOp {
                left: Box::new(base),
                op: BinOp::Pow,
                right: Box::new(exp),
            })
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                })
            }
            Token::Not => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            }
            Token::Tilde => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                })
            }
            // & (address-of) — Go interop, treated as identity in Nebula
            Token::Ampersand => {
                self.advance();
                let operand = self.parse_unary()?;
                // Wrap in a special call so codegen can emit &
                Ok(Expr::Call {
                    callee: Box::new(Expr::Ident("__addr".to_string())),
                    args: vec![crate::ast::CallArg { name: None, value: operand }],
                })
            }
            // * (dereference) — Go interop
            Token::Star => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Call {
                    callee: Box::new(Expr::Ident("__deref".to_string())),
                    args: vec![crate::ast::CallArg { name: None, value: operand }],
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    if matches!(self.peek(), Token::LeftParen) {
                        self.advance();
                        let args = self.parse_call_args()?;
                        self.expect(&Token::RightParen)?;
                        expr = Expr::MethodCall {
                            object: Box::new(expr),
                            method: field,
                            args,
                        };
                    } else {
                        expr = Expr::FieldAccess {
                            object: Box::new(expr),
                            field,
                        };
                    }
                }
                Token::LeftParen => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(&Token::RightParen)?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                Token::LeftBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&Token::RightBracket)?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Token::Question => {
                    self.advance();
                    expr = Expr::Try(Box::new(expr));
                }
                // Multi-line chaining: newline followed by `.` continues the expression
                Token::Newline => {
                    // Peek past the newline(s) to see if a `.` follows
                    let mut lookahead = self.pos + 1;
                    while lookahead < self.tokens.len()
                        && matches!(self.tokens[lookahead].token, Token::Newline)
                    {
                        lookahead += 1;
                    }
                    if lookahead < self.tokens.len()
                        && matches!(self.tokens[lookahead].token, Token::Dot)
                    {
                        // Skip all the newlines — the `.` will be handled next iteration
                        while matches!(self.peek(), Token::Newline) {
                            self.advance();
                        }
                        continue;
                    }
                    break;
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<CallArg>, ParseError> {
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RightParen) {
            if !args.is_empty() {
                self.expect(&Token::Comma)?;
                self.skip_newlines();
            }
            if matches!(self.peek(), Token::RightParen) {
                break;
            }

            // Try named argument: name: value
            let saved = self.pos;
            if let Token::Ident(name) = self.peek().clone() {
                self.advance();
                if matches!(self.peek(), Token::Colon) {
                    self.advance();
                    let value = self.parse_expr()?;
                    args.push(CallArg {
                        name: Some(name),
                        value,
                    });
                    continue;
                }
                self.pos = saved;
            }

            let value = self.parse_expr()?;
            args.push(CallArg { name: None, value });
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::IntLit(n))
            }
            Token::Float(f) => {
                self.advance();
                Ok(Expr::FloatLit(f))
            }
            Token::Bool(b) => {
                self.advance();
                Ok(Expr::BoolLit(b))
            }
            Token::String(s) => {
                self.advance();
                Ok(Expr::StringLit(s))
            }
            Token::Char(c) => {
                self.advance();
                Ok(Expr::CharLit(c))
            }
            Token::Nil => {
                self.advance();
                Ok(Expr::Nil)
            }
            Token::SelfValue => {
                self.advance();
                Ok(Expr::Ident("sf".to_string()))
            }
            Token::Ident(name) => {
                self.advance();
                // Check for struct literal: Name { ... }
                // Suppressed in condition contexts (if/for/while) to avoid ambiguity with blocks
                if !self.no_struct_literal && matches!(self.peek(), Token::LeftBrace) {
                    // Only if the name starts with uppercase (convention for types)
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                        return self.parse_struct_literal(name);
                    }
                }
                Ok(Expr::Ident(name))
            }

            // Parenthesized expression or tuple
            Token::LeftParen => {
                self.advance();
                if matches!(self.peek(), Token::RightParen) {
                    self.advance();
                    return Ok(Expr::Tuple(Vec::new()));
                }
                let first = self.parse_expr()?;
                if matches!(self.peek(), Token::Comma) {
                    // Tuple
                    let mut elements = vec![first];
                    while matches!(self.peek(), Token::Comma) {
                        self.advance();
                        if matches!(self.peek(), Token::RightParen) {
                            break;
                        }
                        elements.push(self.parse_expr()?);
                    }
                    self.expect(&Token::RightParen)?;
                    Ok(Expr::Tuple(elements))
                } else {
                    self.expect(&Token::RightParen)?;
                    Ok(first)
                }
            }

            // List literal
            Token::LeftBracket => {
                self.advance();
                let mut elements = Vec::new();
                self.skip_newlines();
                while !matches!(self.peek(), Token::RightBracket) {
                    if !elements.is_empty() {
                        self.expect(&Token::Comma)?;
                        self.skip_newlines();
                    }
                    if matches!(self.peek(), Token::RightBracket) {
                        break;
                    }
                    elements.push(self.parse_expr()?);
                    self.skip_newlines();
                }
                self.expect(&Token::RightBracket)?;
                Ok(Expr::List(elements))
            }

            // Lambda: |params| body
            Token::Pipe => {
                self.advance();
                let mut params = Vec::new();
                while !matches!(self.peek(), Token::Pipe) {
                    if !params.is_empty() {
                        self.expect(&Token::Comma)?;
                    }
                    let name = self.expect_ident()?;
                    let type_ann = if matches!(self.peek(), Token::Colon) {
                        self.advance();
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    params.push(Param {
                        name,
                        type_ann,
                        default: None,
                        is_variadic: false,
                    });
                }
                self.expect(&Token::Pipe)?;
                let body = if matches!(self.peek(), Token::LeftBrace) {
                    let block = self.parse_block()?;
                    if block.statements.is_empty() {
                        if let Some(expr) = block.expr {
                            *expr
                        } else {
                            Expr::Nil
                        }
                    } else {
                        Expr::Block(block)
                    }
                } else {
                    self.parse_expr()?
                };
                Ok(Expr::Lambda {
                    params,
                    body: Box::new(body),
                })
            }

            // If expression
            Token::If => {
                self.advance();
                self.no_struct_literal = true;
                let condition = self.parse_expr()?;
                self.no_struct_literal = false;
                self.skip_newlines();
                let then_block = self.parse_block()?;
                self.skip_newlines();
                let else_block = if matches!(self.peek(), Token::Else) {
                    self.advance();
                    self.skip_newlines();
                    if matches!(self.peek(), Token::If) {
                        // else if => wrap in block with single if expr
                        let if_expr = self.parse_primary()?;
                        Some(Block {
                            statements: Vec::new(),
                            expr: Some(Box::new(if_expr)),
                        })
                    } else {
                        Some(self.parse_block()?)
                    }
                } else {
                    None
                };
                Ok(Expr::If {
                    condition: Box::new(condition),
                    then_block,
                    else_block,
                })
            }

            // Match expression
            Token::Match => {
                self.advance();
                let subject = self.parse_expr()?;
                self.skip_newlines();
                self.expect(&Token::LeftBrace)?;
                self.skip_newlines();

                let mut arms = Vec::new();
                while !matches!(self.peek(), Token::RightBrace) {
                    let pattern = self.parse_pattern()?;
                    let guard = if matches!(self.peek(), Token::If) {
                        self.advance();
                        Some(self.parse_expr()?)
                    } else {
                        None
                    };
                    self.expect(&Token::FatArrow)?;
                    let body = self.parse_expr()?;
                    arms.push(MatchArm {
                        pattern,
                        guard,
                        body,
                    });
                    self.skip_newlines();
                }
                self.expect(&Token::RightBrace)?;

                Ok(Expr::Match {
                    subject: Box::new(subject),
                    arms,
                })
            }

            // For expression
            Token::For => {
                self.advance();
                let first_binding = self.expect_ident()?;
                let (index_binding, binding) = if matches!(self.peek(), Token::Comma) {
                    self.advance();
                    let second = self.expect_ident()?;
                    (Some(first_binding), second)
                } else {
                    (None, first_binding)
                };
                self.expect(&Token::In)?;
                self.no_struct_literal = true;
                let iterable = self.parse_expr()?;
                self.no_struct_literal = false;
                self.skip_newlines();
                let body = self.parse_block()?;
                Ok(Expr::For {
                    binding,
                    index_binding,
                    iterable: Box::new(iterable),
                    body,
                })
            }

            // While expression
            Token::While => {
                self.advance();
                self.no_struct_literal = true;
                let condition = self.parse_expr()?;
                self.no_struct_literal = false;
                self.skip_newlines();
                let body = self.parse_block()?;
                Ok(Expr::While {
                    condition: Box::new(condition),
                    body,
                })
            }

            // Loop expression
            Token::Loop => {
                self.advance();
                self.skip_newlines();
                let body = self.parse_block()?;
                Ok(Expr::Loop { body })
            }

            // Try/catch: try { ... } ch e { ... }
            Token::Try => {
                self.advance();
                self.skip_newlines();
                let try_block = self.parse_block()?;
                self.skip_newlines();
                self.expect(&Token::Catch)?;
                let error_binding = self.expect_ident()?;
                self.skip_newlines();
                let catch_block = self.parse_block()?;
                Ok(Expr::TryCatch {
                    try_block,
                    error_binding,
                    catch_block,
                })
            }

            // Block expression
            Token::LeftBrace => {
                // Disambiguate map literal vs block:
                // If { is followed by } it's an empty map/block (treat as empty map)
                // If { expr : (not ::) ... it's a map literal
                // Otherwise it's a block
                if self.is_map_literal() {
                    self.parse_map_literal()
                } else {
                    let block = self.parse_block()?;
                    Ok(Expr::Block(block))
                }
            }

            other => {
                let (line, col) = self.current_span();
                Err(ParseError {
                    message: format!("Unexpected token: {other:?}"),
                    line,
                    col,
                })
            }
        }
    }

    fn parse_struct_literal(&mut self, name: String) -> Result<Expr, ParseError> {
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();
        let mut fields = Vec::new();
        let mut spread = None;

        while !matches!(self.peek(), Token::RightBrace) {
            if !fields.is_empty() || spread.is_some() {
                self.expect(&Token::Comma)?;
                self.skip_newlines();
            }
            if matches!(self.peek(), Token::RightBrace) {
                break;
            }

            // Spread: ...expr
            if matches!(self.peek(), Token::Ellipsis) {
                self.advance();
                spread = Some(Box::new(self.parse_expr()?));
                self.skip_newlines();
                continue;
            }

            let fname = self.expect_ident()?;
            if matches!(self.peek(), Token::Colon) {
                self.advance();
                let value = self.parse_expr()?;
                fields.push((fname, value));
            } else {
                // Shorthand: field name is also variable name
                fields.push((fname.clone(), Expr::Ident(fname)));
            }
            self.skip_newlines();
        }
        self.expect(&Token::RightBrace)?;

        Ok(Expr::StructLit {
            name,
            fields,
            spread,
        })
    }

    /// Lookahead to determine if `{` starts a map literal.
    /// A map literal looks like: { expr : expr , ... }
    /// We check if, after {, there's a string/ident/number followed by `:` (not `::`)
    fn is_map_literal(&self) -> bool {
        let mut i = self.pos + 1; // skip the {
        // Skip newlines
        while i < self.tokens.len() && matches!(self.tokens[i].token, Token::Newline) {
            i += 1;
        }
        if i >= self.tokens.len() { return false; }
        // Check for empty map: { }
        if matches!(self.tokens[i].token, Token::RightBrace) {
            return true;
        }
        // Check for key : value pattern
        match &self.tokens[i].token {
            Token::String(_) | Token::Int(_) | Token::Float(_) | Token::Bool(_) => {
                if i + 1 < self.tokens.len() {
                    matches!(self.tokens[i + 1].token, Token::Colon)
                } else {
                    false
                }
            }
            Token::Ident(_) => {
                if i + 1 < self.tokens.len() {
                    matches!(self.tokens[i + 1].token, Token::Colon)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn parse_map_literal(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::LeftBrace)?;
        self.skip_newlines();

        let mut entries = Vec::new();
        while !matches!(self.peek(), Token::RightBrace) {
            if !entries.is_empty() {
                self.expect(&Token::Comma)?;
                self.skip_newlines();
            }
            if matches!(self.peek(), Token::RightBrace) {
                break;
            }
            let key = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let value = self.parse_expr()?;
            entries.push((key, value));
            self.skip_newlines();
        }
        self.expect(&Token::RightBrace)?;
        Ok(Expr::Map(entries))
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.peek().clone() {
            Token::Ident(name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Token::Ident(name) => {
                self.advance();
                // Check for variant pattern: Name.Variant(...)
                if matches!(self.peek(), Token::Dot) {
                    self.advance();
                    let variant = self.expect_ident()?;
                    let full = format!("{name}.{variant}");
                    if matches!(self.peek(), Token::LeftParen) {
                        self.advance();
                        let mut sub = Vec::new();
                        while !matches!(self.peek(), Token::RightParen) {
                            if !sub.is_empty() {
                                self.expect(&Token::Comma)?;
                            }
                            sub.push(self.parse_pattern()?);
                        }
                        self.expect(&Token::RightParen)?;
                        Ok(Pattern::Variant(full, sub))
                    } else {
                        Ok(Pattern::Variant(full, Vec::new()))
                    }
                } else if matches!(self.peek(), Token::LeftParen) {
                    // Variant(...)
                    self.advance();
                    let mut sub = Vec::new();
                    while !matches!(self.peek(), Token::RightParen) {
                        if !sub.is_empty() {
                            self.expect(&Token::Comma)?;
                        }
                        sub.push(self.parse_pattern()?);
                    }
                    self.expect(&Token::RightParen)?;
                    Ok(Pattern::Variant(name, sub))
                } else if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    // Uppercase identifier → treat as value/enum variant pattern
                    Ok(Pattern::Literal(Expr::Ident(name)))
                } else {
                    Ok(Pattern::Binding(name))
                }
            }
            Token::Int(_) | Token::Float(_) | Token::Bool(_) | Token::String(_) | Token::Char(_) | Token::Nil => {
                let expr = self.parse_primary()?;
                // Check for range pattern
                if matches!(self.peek(), Token::DotDot | Token::DotDotEq) {
                    let inclusive = matches!(self.peek(), Token::DotDotEq);
                    self.advance();
                    let end = self.parse_primary()?;
                    Ok(Pattern::Range(Box::new(expr), Box::new(end), inclusive))
                } else {
                    Ok(Pattern::Literal(expr))
                }
            }
            _ => {
                let (line, col) = self.current_span();
                Err(ParseError {
                    message: format!("Expected pattern, got {:?}", self.peek()),
                    line,
                    col,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;

    fn parse_source(source: &str) -> Program {
        let tokens = lexer::tokenize(source).unwrap();
        parse(tokens).unwrap()
    }

    fn parse_err(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        parse(tokens).unwrap_err().message
    }

    #[test]
    fn test_let_statement() {
        let prog = parse_source("lt x = 42");
        assert_eq!(prog.statements.len(), 1);
        assert!(matches!(&prog.statements[0], Statement::Let { name, .. } if name == "x"));
    }

    #[test]
    fn test_mut_statement() {
        let prog = parse_source("mt count = 0");
        assert!(matches!(&prog.statements[0], Statement::Mut { name, .. } if name == "count"));
    }

    #[test]
    fn test_fn_decl() {
        let prog = parse_source("fn add(a: Int, b: Int) -> Int {\n  a + b\n}");
        assert!(matches!(&prog.statements[0], Statement::FnDecl { name, params, .. }
            if name == "add" && params.len() == 2));
    }

    #[test]
    fn test_struct_decl() {
        let prog = parse_source("st Point {\n  x: Float\n  y: Float\n}");
        assert!(matches!(&prog.statements[0], Statement::StructDecl { name, fields, .. }
            if name == "Point" && fields.len() == 2));
    }

    #[test]
    fn test_if_expression() {
        let prog = parse_source("if x > 0 { 1 } el { -1 }");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::If { .. })));
    }

    #[test]
    fn test_for_loop() {
        let prog = parse_source("fo x in items {\n  print(x)\n}");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::For { binding, .. })
            if binding == "x"));
    }

    #[test]
    fn test_for_with_index() {
        let prog = parse_source("fo i, x in items {\n  print(i)\n}");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::For { index_binding: Some(idx), binding, .. })
            if idx == "i" && binding == "x"));
    }

    #[test]
    fn test_match_expression() {
        let prog = parse_source("mc x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
        if let Statement::Expr(Expr::Match { arms, .. }) = &prog.statements[0] {
            assert_eq!(arms.len(), 2);
        } else {
            panic!("Expected match expression");
        }
    }

    #[test]
    fn test_lambda() {
        let prog = parse_source("|x| x * 2");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Lambda { .. })));
    }

    #[test]
    fn test_pipe_operator() {
        let prog = parse_source("x |> double");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Pipe { .. })));
    }

    #[test]
    fn test_list_literal() {
        let prog = parse_source("[1, 2, 3]");
        if let Statement::Expr(Expr::List(items)) = &prog.statements[0] {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_range() {
        let prog = parse_source("0..10");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Range { inclusive: false, .. })));
    }

    #[test]
    fn test_range_inclusive() {
        let prog = parse_source("0..=10");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Range { inclusive: true, .. })));
    }

    #[test]
    fn test_struct_literal() {
        let prog = parse_source("Point { x: 1.0, y: 2.0 }");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::StructLit { name, .. })
            if name == "Point"));
    }

    #[test]
    fn test_impl_block() {
        let prog = parse_source("im Point {\n  fn new(x: Float) -> Point {\n    Point { x }\n  }\n}");
        assert!(matches!(&prog.statements[0], Statement::ImplBlock { target, .. }
            if target == "Point"));
    }

    #[test]
    fn test_assignment_operators() {
        let prog = parse_source("x += 1");
        assert!(matches!(&prog.statements[0], Statement::Assignment { op: AssignOp::AddAssign, .. }));
    }

    #[test]
    fn test_use_statement() {
        let prog = parse_source("use std.io");
        assert!(matches!(&prog.statements[0], Statement::Use { path, .. } if path.len() == 2));
    }

    #[test]
    fn test_nil_coalesce() {
        let prog = parse_source("x ?? 0");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::NilCoalesce { .. })));
    }

    #[test]
    fn test_method_call() {
        let prog = parse_source("list.map(f)");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::MethodCall { method, .. })
            if method == "map"));
    }

    #[test]
    fn test_parse_error_message() {
        let msg = parse_err("lt = 5");
        assert!(msg.contains("Expected identifier"));
    }

    #[test]
    fn test_multiline_method_chain() {
        let prog = parse_source("x\n    .map(f)\n    .filter(g)");
        // Should parse as a single expression statement with chained methods
        assert_eq!(prog.statements.len(), 1);
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::MethodCall { method, .. }) if method == "filter"));
    }

    #[test]
    fn test_struct_comma_separated_fields() {
        let prog = parse_source("st Pt { x: Int, y: Int }");
        assert!(matches!(&prog.statements[0], Statement::StructDecl { fields, .. } if fields.len() == 2));
    }

    #[test]
    fn test_map_literal_parse() {
        let prog = parse_source(r#"{"a": 1, "b": 2}"#);
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Map(entries)) if entries.len() == 2));
    }

    #[test]
    fn test_multiline_map_literal() {
        let prog = parse_source("{\n  \"a\": 1,\n  \"b\": 2,\n}");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Map(entries)) if entries.len() == 2));
    }

    #[test]
    fn test_zero_arg_lambda() {
        let prog = parse_source("|| 42");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Lambda { params, .. }) if params.is_empty()));
    }

    #[test]
    fn test_semicolons_as_statement_separators() {
        let prog = parse_source("lt x = 1; lt y = 2; x + y");
        assert_eq!(prog.statements.len(), 3);
    }

    #[test]
    fn test_long_keyword_let() {
        // Long-form keywords are now accepted (Nebula 1.0)
        let prog = parse_source("let x = 42");
        assert!(matches!(&prog.statements[0], Statement::Let { name, .. } if name == "x"));
    }

    #[test]
    fn test_long_keyword_for() {
        // Long-form "for" is now accepted
        let prog = parse_source("for x in [1, 2, 3] { x }");
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::For { .. })));
    }

    #[test]
    fn test_long_keyword_struct() {
        // Long-form "struct" is now accepted
        let prog = parse_source("struct Pt { x: Int }");
        assert!(matches!(&prog.statements[0], Statement::StructDecl { name, .. } if name == "Pt"));
    }

    #[test]
    fn test_type_alias() {
        let prog = parse_source("ty UserId = Int");
        assert!(matches!(&prog.statements[0], Statement::TypeAlias { name, .. } if name == "UserId"));
    }
}

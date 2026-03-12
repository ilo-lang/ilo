#![warn(clippy::all)]

pub mod ast;
pub mod builtins;
pub mod codegen;
pub mod diagnostic;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod tools;
pub mod graph;
pub mod verify;
pub mod vm;

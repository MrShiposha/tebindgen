#![deny(clippy::all)]

extern crate proc_macro2;

use std::path::Path;

#[derive(Debug)]
pub enum Symbol<'a> {
    Function(FnSignature<'a>),
    Struct(Struct<'a>),
    Variable(Variable<'a>),
}

#[derive(Debug)]
pub struct FnSignature<'a> {
    name: String,
    ctype: clang::Type<'a>,
    parameters: Vec<FnParameter<'a>>,
}

#[derive(Debug)]
pub struct Struct<'a> {
    name: String,
    ctype: clang::Type<'a>,
    fields: Vec<StructField<'a>>,
}

#[derive(Debug)]
pub struct Variable<'a> {
    name: String,
    ctype: clang::Type<'a>,
}

pub type StructField<'a> = Variable<'a>;
pub type FnParameter<'a> = Variable<'a>;

#[derive(Debug)]
pub struct TranslationUnit {
    name: String,
    tokens: proc_macro2::TokenStream,
}

impl<'a> FnSignature<'a> {
    pub fn new(
        name: String,
        ctype: clang::Type<'a>,
        parameters: Vec<FnParameter<'a>>,
    ) -> FnSignature<'a> {
        FnSignature {
            name,
            ctype,
            parameters,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn ctype(&self) -> &clang::Type<'a> {
        &self.ctype
    }

    pub fn parameters(&self) -> &Vec<FnParameter<'a>> {
        &self.parameters
    }

    pub fn result_type(&self) -> clang::Type<'a> {
        self.ctype().get_result_type().unwrap()
    }
}

impl<'a> Struct<'a> {
    pub fn new(name: String, ctype: clang::Type<'a>, fields: Vec<StructField<'a>>) -> Struct<'a> {
        Struct {
            name,
            ctype,
            fields,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn ctype(&self) -> &clang::Type<'a> {
        &self.ctype
    }

    pub fn fields(&self) -> &Vec<StructField<'a>> {
        &self.fields
    }
}

impl<'a> Variable<'a> {
    pub fn new(name: String, ctype: clang::Type<'a>) -> Variable<'a> {
        Variable { name, ctype }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn ctype(&self) -> &clang::Type<'a> {
        &self.ctype
    }
}

impl TranslationUnit {
    pub fn new<T: AsRef<Path>>(
        file_name: T,
        tokens: Vec<proc_macro2::TokenStream>,
    ) -> TranslationUnit {
        let tokens = quote! {
            #(#tokens)*
        };

        TranslationUnit {
            name: file_name
                .as_ref()
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap(),

            tokens,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn tokens(&self) -> &proc_macro2::TokenStream {
        &self.tokens
    }
}

extern crate proc_macro2;

use std::fs;
use std::path::Path;

#[derive(Debug)]
pub enum Symbol<'a> {
    Function(FnSignature<'a>)
}

#[derive(Debug)]
pub struct FnParameter<'a> {
    name: String,
    ctype: clang::Type<'a>
}

#[derive(Debug)]
pub struct FnSignature<'a> {
    name: String,
    ctype: clang::Type<'a>,
    parameters: Vec<FnParameter<'a>>
}

#[derive(Debug)]
pub struct TranslationUnit {
    name: String,
    tokens: proc_macro2::TokenStream,
}

impl<'a> FnParameter<'a> {
    pub fn new(name: String, ctype: clang::Type<'a>) -> FnParameter<'a> {
        FnParameter {
            name: name,
            ctype: ctype
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn ctype(&self) -> &clang::Type<'a> {
        &self.ctype
    }
}

impl<'a> FnSignature<'a> {
    pub fn new(name: String, ctype: clang::Type<'a>, parameters: Vec<FnParameter<'a>>)
        -> FnSignature<'a> {
        FnSignature {
            name: name,
            ctype: ctype,
            parameters: parameters
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
}

impl TranslationUnit {
    pub fn new<T: AsRef<Path>>(file_name: T, tokens: Vec<proc_macro2::TokenStream>) -> TranslationUnit {
        let tokens = quote! {
            #(#tokens)*
        };

        TranslationUnit {
            name: file_name.as_ref()
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap(),
                
            tokens: tokens
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn tokens(&self) -> &proc_macro2::TokenStream {
        &self.tokens
    }

    // pub fn write_to_file<T: AsRef<Path>>(out_file: T) {
    // }
}
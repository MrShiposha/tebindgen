extern crate clang;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate quote;
extern crate proc_macro2;

use std::collections::HashSet;
use std::path::Path;
use std::fs;
use std::env;

mod ir;

lazy_static! {
    static ref CLANG: clang::Clang = clang::Clang::new().unwrap();
}

type UserGeneratorFn = dyn Fn(ir::Symbol) -> proc_macro2::TokenStream;

pub struct Generator<'a> {
    index: clang::Index<'a>,
    symbols: HashSet<String>
}

impl<'a> Generator<'a> {
    pub fn new() -> Generator<'a> {
        Generator {
            index: clang::Index::new(&CLANG, false, false),
            symbols: HashSet::new()
        }
    }

    pub fn generate<T>(&mut self, dir: T, user_gen: &UserGeneratorFn)
        -> Vec<ir::TranslationUnit>
        where T: AsRef<Path> {
        let dir = dir.as_ref();

        let mut units = vec![];

        if dir.is_dir() {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();

                if path.is_dir() {
                    self.generate(path, user_gen);
                } else {
                    let ext = path.extension();
                    if ext.is_some() && ext.unwrap() == "c" {
                        let unit = self.generate_for_file(path, user_gen);

                        units.push(unit);
                    }
                }
            }
        }

        units
    }

    fn generate_for_file<T: AsRef<Path>>(&mut self, file: T, user_gen: &UserGeneratorFn) 
        -> ir::TranslationUnit {
        let file = file.as_ref();
        assert!(file.is_file());

        let mut tokens = vec![];
        macro_rules! try_add_tokens {
            ($($code:tt)*) => {
                if let Some(new_tokens) = $($code)* {
                    tokens.push(new_tokens);
                }
            };
        }

        // let cwd = env::current_dir().unwrap();
        // let includes_dir = cwd;
        // let include_option = "-I".to_string() + includes_dir.as_os_str().to_str().unwrap();
        // let include_option = include_option.as_str();

        // let unit = Generator::get_translation_unit(&self.index, file);

        // let nodes = unit.get_entity().get_children();
        // for node in nodes {
        //     match node.get_kind() {
        //         clang::EntityKind::FunctionDecl => {
        //             try_add_tokens![self.generate_fn(node, user_gen)];
        //         },
        //         _ => {}
        //     }
        // }

        ir::TranslationUnit::new(file, tokens)
    }

    fn get_translation_unit<T: AsRef<Path>>(index: &'a clang::Index, file: T) 
        -> clang::TranslationUnit<'a> {
        index.parser(file.as_ref())
        .keep_going(true)
        .skip_function_bodies(true)
        // .arguments(&[include_option])
        .parse()
        .unwrap()
    } 

    fn generate_fn(&mut self, fn_decl: clang::Entity, user_gen: &UserGeneratorFn) 
        -> Option<proc_macro2::TokenStream> {
        let fn_type = fn_decl.get_type().unwrap();
        let fn_name = fn_decl.get_name().unwrap();
        let mut parameters = vec![];

        if self.symbols.contains(&fn_name) {
            return None;
        }

        let mut is_exported = false;
        for child in fn_decl.get_children() {
            match child.get_kind() {
                clang::EntityKind::ParmDecl => {
                    let prm_type = child.get_type().unwrap();
                    let prm_name = child.get_name().unwrap_or("".to_string());

                    parameters.push(ir::FnParameter::new(prm_name, prm_type));
                },

                clang::EntityKind::DllExport => is_exported = true,
                _ => {}
            }
        }

        if is_exported {
            self.symbols.insert(fn_name.clone());

            let signature = ir::FnSignature::new(fn_name, fn_type, parameters);
            let symbol = ir::Symbol::Function(signature);

            Option::from(user_gen(symbol))
        } else {
            None
        }
    }
}
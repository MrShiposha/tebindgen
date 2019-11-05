#![feature(matches_macro)]

extern crate clang;

#[macro_use]
extern crate quote;
extern crate proc_macro2;

#[macro_use]
extern crate lazy_static;

use std::collections::HashSet;
use std::path::Path;
use std::fs;

mod ir;

pub struct Generator {
    symbols: HashSet<String>,
    arguments: Vec<String>
}

impl Generator {
    pub fn new() -> Generator {
        Generator {
            symbols: HashSet::new(),
            arguments: vec![]
        }
    }

    pub fn generate<Dir, Gen>(&mut self, dir: Dir, user_gen: Gen)
        -> Vec<ir::TranslationUnit> 
        where
            Dir: AsRef<Path>, 
            Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream> {
        lazy_static! {
            static ref CLANG: clang::Clang = clang::Clang::new().expect("Unable to initialize clang");
        };
        
        let index = clang::Index::new(&CLANG, false, false);        
        let units = self.generate_helper(dir, &user_gen, &index);

        units
    }

    pub fn generate_helper<'a, Dir, Gen>(
        &mut self, 
        dir: Dir, 
        user_gen: &Gen,
        index: &clang::Index<'a>
    ) -> Vec<ir::TranslationUnit>
        where
            Dir: AsRef<Path>, 
            Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream> {
        let dir = dir.as_ref();

        let mut units = vec![];

        if dir.is_dir() {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();

                if path.is_dir() {
                    self.generate_helper(path, user_gen, index);
                } else {
                    let ext = path.extension();
                    if ext.is_some() && ext.unwrap() == "c" {
                        let unit = self.generate_for_file(index, path, user_gen);

                        units.push(unit);
                    }
                }
            }
        }

        units
    }

    pub fn c_flag<T: Into<String>>(&mut self, flag: T) -> &mut Self {
        self.arguments.push(flag.into());

        self
    }

    pub fn include_directory<T: AsRef<Path>>(&mut self, dir: T) -> &mut Self {
        let dir = dir.as_ref()
            .as_os_str()
            .to_str()
            .expect("Unable to convert include directory path to unicode string");

        let c_flag = String::from("-I") + dir;
        self.c_flag(c_flag);

        self
    }

    pub fn system_include_directory<T: AsRef<Path>>(&mut self, dir: T) -> &mut Self {
        let dir = dir.as_ref()
            .as_os_str()
            .to_str()
            .expect("Unable to convert system include directory path to unicode string");

        let c_flag = String::from("-isystem ") + dir;
        self.c_flag(c_flag);

        self
    }

    pub fn define<T: AsRef<str>>(&mut self, c_macro: T) -> &mut Self {
        let c_flag = String::from("-D") + c_macro.as_ref();
        self.c_flag(c_flag);

        self
    }

    pub fn define_value<T: AsRef<str>>(&mut self, c_macro: T, macro_value: T) -> &mut Self {
        let c_flag = String::from("-D") + c_macro.as_ref() + "=" + macro_value.as_ref();
        self.c_flag(c_flag);

        self
    }

    fn generate_for_file<File, Gen>(&mut self, index: &clang::Index, file: File, user_gen: &Gen)
        -> ir::TranslationUnit
        where
            File: AsRef<Path>,
            Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream> {
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

        let unit = self.get_translation_unit(index, file);

        let nodes = unit.get_entity().get_children();
        for node in nodes {
            match node.get_kind() {
                clang::EntityKind::FunctionDecl => {
                    try_add_tokens![self.generate_fn(node, user_gen)];
                },
                _ => {}
            }
        }

        ir::TranslationUnit::new(file, tokens)
    }

    fn get_translation_unit<'a, T: AsRef<Path>>(&self, index: &'a clang::Index, file: T) 
        -> clang::TranslationUnit<'a> {
        index.parser(file.as_ref())
        .keep_going(true)
        .skip_function_bodies(true)
        .arguments(&self.arguments)
        .parse()
        .unwrap()
    } 

    fn generate_fn<Gen>(&mut self, fn_decl: clang::Entity, user_gen: &Gen)
        -> Option<proc_macro2::TokenStream>
        where Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream> {
        let fn_type = fn_decl.get_type().unwrap();
        let fn_name = fn_decl.get_name().unwrap();
        let mut parameters = vec![];

        if self.symbols.contains(&fn_name) {
            return None;
        }

        #[allow(unused_mut)]
        let mut is_exported = {
            #[cfg(target_family = "unix")] {
                match fn_decl.get_visibility() {
                    Some(visibility) => visibility == clang::Visibility::Default,
                    None => false
                }
            }

            #[cfg(not(target_family = "unix"))] {
                false
            }
        };

        for child in fn_decl.get_children() {
            match child.get_kind() {
                clang::EntityKind::ParmDecl => {
                    let prm_type = child.get_type().unwrap();
                    let prm_name = child.get_name().unwrap_or("".to_string());

                    parameters.push(ir::FnParameter::new(prm_name, prm_type));
                },

                #[cfg(target_family = "windows")]
                clang::EntityKind::DllExport => is_exported = true,
                _ => {}
            }
        }

        if is_exported {
            self.symbols.insert(fn_name.clone());

            let signature = ir::FnSignature::new(fn_name, fn_type, parameters);
            let symbol = ir::Symbol::Function(signature);

            user_gen(symbol)
        } else {
            None
        }
    }
}


#[cfg(test)]
mod tests { 
    use super::*;
    use std::path::PathBuf;
    use std::env;

    lazy_static! {
        static ref DATA: PathBuf = env::current_dir()
            .unwrap()
            .join("src")
            .join("testdata");
    }

    #[test]
    fn test_include_flag() {
        let include_test_dir = DATA.clone().as_path().join("indlue_test");
        let include_path = include_test_dir.clone().join("includes");

        Generator::new()
            .include_directory(include_path)
            .generate(include_test_dir, |symbol| {
                match symbol {
                    ir::Symbol::Function(signature) => {
                        assert_eq!(signature.name(), "include_test_fn");
                        assert_eq!(signature.result_type().get_display_name(), "void *");

                        let parameter = &signature.parameters()[0];
                        assert_eq!(parameter.name(), "test_param");
                        assert_eq!(parameter.ctype().get_display_name(), "int");

                        assert_eq!(signature.ctype().get_display_name(), "void *(int)")
                    },
                    _ => {}
                }
                None
            });
    }

    #[test]
    fn test_define_flag() {
        let define_test_dir = DATA.clone().as_path().join("define_test");

        Generator::new()
            .define("TEST_MACRO")
            .generate(define_test_dir, |symbol| {
                match symbol {
                    ir::Symbol::Function(signature) => {
                        assert_eq!(signature.name(), "define_test_fn");
                        assert_eq!(signature.result_type().get_display_name(), "int");

                        let parameter = &signature.parameters()[0];
                        assert_eq!(parameter.name(), "test_param");
                        assert_eq!(parameter.ctype().get_display_name(), "const char *");

                        assert_eq!(signature.ctype().get_display_name(), "int (const char *)");
                    },
                    _ => {}
                }
                None
            });
    }

    #[test]
    fn test_define_value_flag() {
        let define_value_test_dir = DATA.clone().as_path().join("define_value_test");

        Generator::new()
            .define_value("RETURN_T", "double")
            .define_value("STRING_T", "const char *")
            .generate(define_value_test_dir, |symbol| {
                match symbol {
                    ir::Symbol::Function(signature) => {
                        assert_eq!(signature.name(), "define_value_test_fn");
                        assert_eq!(signature.result_type().get_display_name(), "double");

                        let parameter = &signature.parameters()[0];
                        assert_eq!(parameter.name(), "test_param");
                        assert_eq!(parameter.ctype().get_display_name(), "const char *");

                        assert_eq!(signature.ctype().get_display_name(), "double (const char *)");
                    },
                    _ => {}
                }
                None
            });
    }
}
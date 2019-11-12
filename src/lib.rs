extern crate clang;

#[macro_use]
extern crate quote;
extern crate proc_macro2;

#[macro_use]
extern crate lazy_static;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

mod ir;

type SymbolName = String;
type StructName = String;
type HasFields = bool;

#[derive(Default)]
pub struct Generator {
    symbols: HashSet<SymbolName>,
    structs: HashMap<StructName, HasFields>,
    arguments: Vec<String>,
}

impl Generator {
    pub fn new() -> Generator {
        Self::default()
    }

    pub fn generate<Dir, Gen>(&mut self, dir: Dir, user_gen: Gen) -> Vec<ir::TranslationUnit>
    where
        Dir: AsRef<Path>,
        Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream>,
    {
        lazy_static! {
            static ref CLANG: clang::Clang =
                clang::Clang::new().expect("Unable to initialize clang");
        };

        let index = clang::Index::new(&CLANG, false, false);
        self.generate_units_helper(dir, &user_gen, &index)
    }

    pub fn generate_units_helper<'a, Dir, Gen>(
        &mut self,
        dir: Dir,
        user_gen: &Gen,
        index: &clang::Index<'a>,
    ) -> Vec<ir::TranslationUnit>
    where
        Dir: AsRef<Path>,
        Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream>,
    {
        let dir = dir.as_ref();

        let mut units = vec![];

        if dir.is_dir() {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();

                if path.is_dir() {
                    self.generate_units_helper(path, user_gen, index);
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

    pub fn c_flags<T: AsRef<str>>(&mut self, flags: &[T]) -> &mut Self {
        let mut flags: Vec<String> = flags.iter().map(|f| String::from(f.as_ref())).collect();
        self.arguments.append(&mut flags);

        self
    }

    pub fn include_directory<T: AsRef<Path>>(&mut self, dir: T) -> &mut Self {
        let dir = dir
            .as_ref()
            .as_os_str()
            .to_str()
            .expect("Unable to convert include directory path to unicode string");

        let c_flag = String::from("-I") + dir;
        self.c_flag(c_flag);

        self
    }

    pub fn system_include_directory<T: AsRef<Path>>(&mut self, dir: T) -> &mut Self {
        let dir = dir
            .as_ref()
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

    pub fn clear_arguments(&mut self) -> &mut Self {
        self.arguments.clear();
        self
    }

    fn generate_for_file<File, Gen>(
        &mut self,
        index: &clang::Index,
        file: File,
        user_gen: &Gen,
    ) -> ir::TranslationUnit
    where
        File: AsRef<Path>,
        Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream>,
    {
        let file = file.as_ref();
        assert!(file.is_file());

        let mut tokens = vec![];
        macro_rules! try_add_tokens {
            ($expr:expr) => {
                if let Some(new_tokens) = $expr {
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
                }
                clang::EntityKind::StructDecl => {
                    try_add_tokens![self.generate_struct(node, user_gen)];
                }
                clang::EntityKind::VarDecl => {
                    try_add_tokens![self.generate_var(node, user_gen)];
                }
                _ => {}
            }
        }

        ir::TranslationUnit::new(file, tokens)
    }

    fn get_translation_unit<'a, T: AsRef<Path>>(
        &self,
        index: &'a clang::Index,
        file: T,
    ) -> clang::TranslationUnit<'a> {
        index
            .parser(file.as_ref())
            .keep_going(true)
            .skip_function_bodies(true)
            .arguments(&self.arguments)
            .parse()
            .unwrap()
    }

    fn generate_fn<Gen>(
        &mut self,
        fn_decl: clang::Entity,
        user_gen: &Gen,
    ) -> Option<proc_macro2::TokenStream>
    where
        Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream>,
    {
        let fn_type = fn_decl.get_type().unwrap();
        let fn_name = fn_decl.get_name().unwrap();
        let mut parameters = vec![];

        if self.symbols.contains(&fn_name) {
            return None;
        }

        #[allow(unused_mut)]
        let mut is_exported = {
            #[cfg(target_family = "unix")]
            {
                match fn_decl.get_visibility() {
                    Some(v) => v == clang::Visibility::Default,
                    None => false,
                }
            }

            #[cfg(not(target_family = "unix"))]
            {
                false
            }
        };

        for child in fn_decl.get_children() {
            #[allow(clippy::single_match)]
            match child.get_kind() {
                clang::EntityKind::ParmDecl => {
                    let prm_type = child.get_type().unwrap();
                    let prm_name = child.get_name().unwrap_or_default();

                    parameters.push(ir::FnParameter::new(prm_name, prm_type));
                }

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

    fn generate_struct<Gen>(
        &mut self,
        struct_decl: clang::Entity,
        user_gen: &Gen,
    ) -> Option<proc_macro2::TokenStream>
    where
        Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream>,
    {
        let struct_name = struct_decl.get_name().unwrap();
        let struct_type = struct_decl.get_type().unwrap();
        let mut fields = vec![];

        if *self.structs.get(&struct_name).unwrap_or(&false) {
            return None;
        }

        for child in struct_decl.get_children() {
            #[allow(clippy::single_match)]
            match child.get_kind() {
                clang::EntityKind::FieldDecl => {
                    let field =
                        ir::StructField::new(child.get_name().unwrap(), child.get_type().unwrap());

                    fields.push(field);
                },
                _ => {}
            }
        }

        self.structs.insert(struct_name.clone(), !fields.is_empty());

        let struct_obj = ir::Struct::new(struct_name, struct_type, fields);
        let symbol = ir::Symbol::Struct(struct_obj);

        user_gen(symbol)
    }

    fn generate_var<Gen>(
        &mut self,
        var_decl: clang::Entity,
        user_gen: &Gen,
    ) -> Option<proc_macro2::TokenStream>
    where
        Gen: Fn(ir::Symbol) -> Option<proc_macro2::TokenStream>,
    {
        let var_name = var_decl.get_name().unwrap();
        let var_type = var_decl.get_type().unwrap();

        if self.symbols.contains(&var_name) {
            return None;
        }

        let is_exported = {
            #[cfg(target_family = "unix")]
            {
                match var_decl.get_visibility() {
                    Some(v) => v == clang::Visibility::Default,
                    None => false,
                }
            }

            #[cfg(target_family = "windows")]
            {
                var_decl
                    .get_children()
                    .iter()
                    .find(|&child| child.get_kind() == clang::EntityKind::DllExport)
                    .is_some()
            }

            #[cfg(all(not(target_family = "unix"), not(target_family = "windows")))]
            {
                unimplemented!()
            }
        };

        if is_exported {
            self.symbols.insert(var_name.clone());

            let var = ir::Variable::new(var_name, var_type);
            let symbol = ir::Symbol::Variable(var);

            user_gen(symbol)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    macro_rules! check_fn_symbol {
        ($signature:expr => {
            name: $fn_name:ident,
            ctype: $($ctype:tt)+
        }) => {{
            let string_signature = String::from(stringify![$($ctype)*]);
            let open_brace_idx = string_signature.find('(').expect("ctype: '(' expected");
            let close_brace_idx = string_signature.rfind(')').expect("ctype: ')' expected");

            let return_type = string_signature[0..open_brace_idx].trim();
            let params = string_signature[open_brace_idx+1..close_brace_idx].trim();
            let params: Vec<&str> = if params.chars().all(|c| c.is_whitespace()) {
                vec![]
            } else {
                params.split(',').collect()
            };

            let mut params_names = vec![];
            let mut params_types = vec![];

            params.iter().for_each(|param| {
                let param: Vec<&str> = param.split(':').collect();
                params_names.push(param[0].trim());
                params_types.push(param[1].trim());
            });

            let mut string_ctype = String::from(return_type);
            if return_type.chars().last().unwrap().is_alphanumeric() {
                string_ctype.push(' ');
            }
            string_ctype.push('(');
            string_ctype.push_str(params_types.join(", ").as_str());
            string_ctype.push(')');

            assert_eq!($signature.name(), stringify![$fn_name]);
            assert_eq!($signature.ctype().get_display_name(), string_ctype);
            assert_eq!($signature.result_type().get_display_name(), return_type);

            if params.is_empty() {
                assert!($signature.parameters().is_empty());
            } else {
                $signature.parameters().iter()
                    .enumerate()
                    .for_each(|(i, param)| {
                        assert_eq!(param.name(), params_names[i]);
                        assert_eq!(param.ctype().get_display_name(), params_types[i]);
                    });
            }
        }};
    }

    macro_rules! check_struct {
        ($struc:expr => $struct_name:ident {
            $($fields:tt)*
        }) => {{
            let struct_name = stringify![$struct_name];
            let string_fields = stringify![$($fields)*];
            let fields: Vec<&str> = if string_fields.chars().all(|c| c.is_whitespace()) {
                vec![]
            } else {
                string_fields.split(',').collect()
            };

            let mut fields_names = vec![];
            let mut fields_types = vec![];

            fields.iter().for_each(|field| {
                let field: Vec<&str> = field.split(':').collect();
                fields_names.push(field[0].trim());
                fields_types.push(field[1].trim());
            });

            assert_eq!($struc.name(), struct_name);
            assert_eq!($struc.ctype().get_display_name(), String::from("struct ") + struct_name);
            if fields.is_empty() {
                assert!($struc.fields().is_empty());
            } else {
                $struc.fields().iter()
                    .enumerate()
                    .for_each(|(i, field)| {
                        assert_eq!(field.name(), fields_names[i]);
                        assert_eq!(field.ctype().get_display_name(), fields_types[i]);
                    });
            }
        }};
    }

    macro_rules! check_var_symbol {
        ($var:expr => $var_name:ident: $($var_type:tt)+) => {{
            let var_name = stringify![$var_name];
            let var_type = stringify![$($var_type)+];

            assert_eq!($var.name(), var_name);
            assert_eq!($var.ctype().get_display_name(), var_type);
        }};
    }

    macro_rules! test_generator {
        ($generator_name:ident($symbol:ident): $($gen:tt)*) => {
            |$symbol| {
                $($gen)*

                Option::from(quote!($generator_name called))
            }
        };
    }

    macro_rules! assert_generator_called {
        ($units:expr, $generator_name:ident) => {{
            assert!{
                $units.len() > 0,
                "No files processed. Please assign DATA variable correct path to res/test folder"
            };

            for (i, unit) in $units.iter().enumerate() {
                let tokens = unit.tokens().to_string();
                let expected = quote!($generator_name called).to_string();

                assert!{
                    tokens == expected,
                    "Unit {} has invalid tokens. \n\tExpected: `{}`\n\tActual: `{}`",
                    i,
                    expected,
                    tokens
                };
            }
        }};
    }

    lazy_static! {
        static ref DATA: PathBuf = env::current_dir().unwrap().join("res").join("test");
    }

    #[test]
    fn test_generate_fn() {
        let generate_fn_test_dir = DATA.clone().as_path().join("generate_fn_test");

        let units = Generator::new().generate(
            generate_fn_test_dir,
            test_generator! {
                generate_fn_gen(symbol): match symbol {
                    ir::Symbol::Function(signature) => {
                        match signature.name() {
                            "fn0" => check_fn_symbol![signature => {
                                name: fn0,
                                ctype: void()
                            }],
                            "fn1" => check_fn_symbol![signature => {
                                name: fn1,
                                ctype: int(: char)
                            }],
                            "fn2" => check_fn_symbol![signature => {
                                name: fn2,
                                ctype: double(arg1: int, arg2: const char *)
                            }],
                            _ => {}
                        }
                    }
                    _ => {}
                }
            },
        );

        assert_generator_called![units, generate_fn_gen];
    }

    #[test]
    fn test_generate_struct() {
        let generate_struct_test_dir = DATA.clone().as_path().join("generate_struct_test");

        let units = Generator::new().generate(
            generate_struct_test_dir,
            test_generator! {
                generate_struct_gen(symbol): match symbol {
                    ir::Symbol::Struct(decl) => {
                        match decl.name() {
                            "Empty" => check_struct![decl => Empty {}],
                            "Fields" => check_struct![decl => Fields {
                                a: int,
                                b: const double *
                            }],
                            "Forward" => check_struct![decl => Forward {}],
                            "FwdFields" => {
                                if decl.fields().is_empty() {
                                    return None;
                                } else {
                                    check_struct![decl => FwdFields {
                                        some: int
                                    }];
                                }
                            },
                            _ => {}
                        }
                    }
                    _ => {}
                }
            },
        );

        assert_generator_called![units, generate_struct_gen];
    }

    #[test]
    fn test_generate_var() {
        let generate_var_test_dir = DATA.clone().as_path().join("generate_var_test");

        let units = Generator::new().generate(
            generate_var_test_dir,
            test_generator! {
                generate_var_gen(symbol): match symbol {
                    ir::Symbol::Variable(var) => check_var_symbol![var => test_var: const char *],
                    _ => {}
                }
            },
        );

        assert_generator_called![units, generate_var_gen];
    }

    #[test]
    fn test_include_flag() {
        let include_test_dir = DATA.clone().as_path().join("include_test");
        let include_path = include_test_dir.clone().join("includes");

        let units = Generator::new().include_directory(include_path).generate(
            include_test_dir,
            test_generator! {
                include_gen(symbol): match symbol {
                    ir::Symbol::Function(signature) => check_fn_symbol![signature => {
                        name: include_test_fn,
                        ctype: void *(test_param: int)
                    }],
                    _ => {}
                }
            },
        );

        assert_generator_called![units, include_gen];
    }

    #[test]
    fn test_define_flag() {
        let define_test_dir = DATA.clone().as_path().join("define_test");

        let units = Generator::new().define("TEST_MACRO").generate(
            define_test_dir,
            test_generator! {
                define_gen(symbol): match symbol {
                    ir::Symbol::Function(signature) => check_fn_symbol![signature => {
                        name: define_test_fn,
                        ctype: int(test_param: const char *)
                    }],
                    _ => {}
                }
            },
        );

        assert_generator_called![units, define_gen];
    }

    #[test]
    fn test_clear_arguments_and_cflags() {
        let mut generator = Generator::new();

        assert!(generator.arguments.is_empty());
        generator.c_flags(&["some_flag1", "some_flag2"]);
        
        assert!(!generator.arguments.is_empty());
        assert_eq!(generator.arguments[0], "some_flag1");
        assert_eq!(generator.arguments[1], "some_flag2");

        generator.clear_arguments();
        assert!(generator.arguments.is_empty());
    }

    #[test]
    fn test_define_value_flag() {
        let define_value_test_dir = DATA.clone().as_path().join("define_value_test");

        let units = Generator::new()
            .define_value("RETURN_T", "double")
            .define_value("STRING_T", "const char *")
            .generate(
                define_value_test_dir,
                test_generator! {
                    define_value_gen(symbol): match symbol {
                        ir::Symbol::Function(signature) => check_fn_symbol![signature => {
                            name: define_value_test_fn,
                            ctype: double(test_param: const char *)
                        }],
                        _ => {}
                    }
                },
            );

        assert_generator_called![units, define_value_gen];
    }

    #[test]
    fn test_hidden() {
        let hidden_test_dir = DATA.clone().as_path().join("hidden");

        let units = Generator::new()
            .generate(
                hidden_test_dir,
                test_generator! {
                    hidden_gen(_symbol): assert!(false, "This must never be called");
                },
            );

        assert_eq!(units.len(), 2);
        assert!(units[0].tokens().to_string().is_empty());
        assert!(units[1].tokens().to_string().is_empty());
    }
}

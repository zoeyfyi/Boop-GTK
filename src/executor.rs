use crate::{scriptmap::Scripts, XDG_DIRS};
use dirty2::Dirty;
use eyre::{Context, ContextCompat, Result};
use rusty_v8 as v8;
use std::{
    cell::RefCell,
    convert::TryFrom,
    env,
    error::Error,
    fmt::{Debug, Display},
    fs::File,
    io::Read,
    rc::Rc,
    sync::Once,
    time::Instant,
};

static BOOP_WRAPPER_START: &str = "
/***********************************
*     Start of Boop's wrapper      *
***********************************/
            
(function() {
    var module = {
        exports: {}
    };
            
    const moduleWrapper = (function (exports, module) {

/***********************************
*      End of Boop's wrapper      *
***********************************/

";

static BOOP_WRAPPER_END: &str = "
            
/***********************************
*     Start of Boop's wrapper      *
***********************************/
            
    }).apply(module.exports, [module.exports, module]);

    return module.exports;
})();
            
/***********************************
*      End of Boop's wrapper      *
***********************************/
";

static INIT_V8: Once = Once::new();

pub struct Executor {
    isolate: v8::OwnedIsolate,
}

impl Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Executor{{}}")
    }
}

struct ExecutorState {
    global_context: Option<v8::Global<v8::Context>>,
    main_function: Option<v8::Global<v8::Function>>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionStatus {
    // true if text was selected when execution began
    is_text_selected: bool,

    info: Option<String>,
    error: Option<String>,

    insert: Vec<String>,
    full_text: Dirty<String>,
    text: Dirty<String>,
    selection: Dirty<String>,
}

impl ExecutionStatus {
    fn reset(&mut self) {
        self.info = None;
        self.error = None;
        self.insert.clear();
        self.full_text.write().clear();
        Dirty::clear(&mut self.full_text);
        self.text.write().clear();
        Dirty::clear(&mut self.text);
    }

    pub fn info(&self) -> Option<&String> {
        self.info.as_ref()
    }

    pub fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }

    pub fn into_replacement(self) -> TextReplacement {
        // not quite sure what the correct behaviour here should be
        // right now the order of presidence is:
        // 0. insertion
        // 1. fullText
        // 2. selection
        // 3. text (with select)
        // 4. text (without selection)
        // TODO: move into ExecutionStatus
        if !self.insert.is_empty() {
            info!("found insertion");
            TextReplacement::Insert(self.insert)
        } else if self.full_text.dirty() {
            info!("found full_text replacement");
            TextReplacement::Full(self.full_text.unwrap())
        } else if self.selection.dirty() {
            info!("found selection replacement");
            TextReplacement::Selection(self.selection.unwrap())
        } else if self.is_text_selected && self.text.dirty() {
            info!("found text (with selection) replacement");
            TextReplacement::Selection(self.text.unwrap())
        } else if self.text.dirty() {
            info!("found text (without selection) replacement");
            TextReplacement::Full(self.text.unwrap())
        } else {
            TextReplacement::None
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TextReplacement {
    Full(String),
    Selection(String),
    Insert(Vec<String>),
    None,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct JSException {
    pub exception_str: String,
    pub resource_name: Option<String>,
    pub source_line: Option<String>,
    pub line_number: Option<usize>,
    pub columns: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutorError {
    SourceExceedsMaxLength,
    Compile(JSException),
    Execute(JSException),
    NoMain,
}

impl Display for ExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutorError::SourceExceedsMaxLength => write!(f, "source exceeds max length"),
            ExecutorError::Compile(exception) => write!(f, "JS compile exception: {:?}", exception),
            ExecutorError::Execute(exception) => {
                write!(f, "JS execution exception: {:?}", exception)
            }
            ExecutorError::NoMain => write!(f, "no main function"),
        }
    }
}

impl Error for ExecutorError {}

impl ExecutorError {
    fn format_exception(exception: JSException) -> String {
        match (exception.line_number, exception.columns) {
            (Some(line_number), Some(columns)) => format!(
                r#"<span foreground="red" weight="bold">EXCEPTION:</span> {} ({}:{} - {}:{})"#,
                exception.exception_str, line_number, columns.0, line_number, columns.1
            ),
            _ => format!(
                r#"<span foreground="red" weight="bold">EXCEPTION:</span> {}"#,
                exception.exception_str,
            ),
        }
    }

    pub fn into_notification_string(self) -> String {
        match self {
            ExecutorError::SourceExceedsMaxLength => {
                String::from(r#"<span foreground="red">ERROR:</span> Script exceeds max length"#)
            }
            ExecutorError::Compile(exception) => ExecutorError::format_exception(exception),
            ExecutorError::Execute(exception) => ExecutorError::format_exception(exception),
            ExecutorError::NoMain => {
                String::from(r#"<span foreground="red">ERROR:</span> No main function"#)
            }
        }
    }
}

impl Executor {
    pub fn new(source: &str) -> eyre::Result<Self> {
        INIT_V8.call_once(|| {
            let start = Instant::now();

            // initialize V8
            let platform = v8::new_default_platform().unwrap();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();

            info!("V8 initialized in {:?}", start.elapsed());
        });

        // set up execution context
        let mut isolate = {
            let start = Instant::now();

            let isolate = v8::Isolate::new(Default::default());
            info!("isolate initialized in {:?}", start.elapsed());

            isolate
        };
        let (global_context, main_function) = {
            let scope = &mut v8::HandleScope::new(&mut isolate);
            // let context = v8::Context::new(scope);
            let (context, main_function) = Executor::initialize_context(source, scope)?;
            (v8::Global::new(scope, context), main_function)
        };

        // set status slot, stores execution infomation
        let status_slot: Rc<RefCell<ExecutionStatus>> =
            Rc::new(RefCell::new(ExecutionStatus::default()));
        isolate.set_slot(status_slot);

        // set state slot, stores v8 details
        let state_slot: Rc<RefCell<ExecutorState>> = Rc::new(RefCell::new(ExecutorState {
            global_context: Some(global_context),
            main_function: Some(main_function),
        }));
        isolate.set_slot(state_slot);

        Ok(Executor { isolate })
    }

    // load source code from internal files or external filesystem depending on the path
    fn load_raw_source(path: String) -> Result<String> {
        if path.starts_with("@boop/") {
            // script is internal

            let internal_path = path.replace("@boop/", "lib/");
            info!(
                "found internal script, real path: #BINARY#/{}",
                internal_path
            );

            let raw_source = String::from_utf8(
                Scripts::get(&internal_path)
                    .ok_or_else(|| eyre!("No internal script with path \"{}\"", path))?
                    .to_vec(),
            )
            .wrap_err("Problem with file encoding")?;

            return Ok(raw_source);
        }

        let mut external_path = if cfg!(test) {
            env::temp_dir()
        } else {
            let mut path = XDG_DIRS.get_config_home();
            path.push("scripts");
            path
        };
        external_path.push(&path);

        info!(
            "found external script, real path: {}",
            external_path.display()
        );

        let mut raw_source = String::new();
        File::open(external_path)
            .wrap_err_with(|| format!("Could not open \"{}\"", path))?
            .read_to_string(&mut raw_source)
            .wrap_err("Problem reading file")?;

        Ok(raw_source)
    }

    fn initialize_context<'s>(
        source: &str,
        scope: &mut v8::HandleScope<'s, ()>,
    ) -> eyre::Result<(v8::Local<'s, v8::Context>, v8::Global<v8::Function>)> {
        let scope = &mut v8::EscapableHandleScope::new(scope);
        let context = v8::Context::new(scope);
        let global = context.global(scope);
        let scope = &mut v8::ContextScope::new(scope, context);

        let require_key =
            v8::String::new(scope, "require").wrap_err("failed to created 'require' string")?;
        let require_val = v8::Function::new(scope, Executor::global_require)
            .wrap_err("failed to created require function")?;
        global.set(scope, require_key.into(), require_val.into());

        // complile and run script
        let code = v8::String::new(scope, source).ok_or(ExecutorError::SourceExceedsMaxLength)?;

        let tc_scope = &mut v8::TryCatch::new(scope);
        let compiled_script = v8::Script::compile(tc_scope, code, None)
            .ok_or_else(|| {
                Executor::extract_exception(tc_scope)
                    .expect("exception occored but no exception was caught")
            })
            .map_err(ExecutorError::Compile)?;

        compiled_script
            .run(tc_scope)
            .ok_or_else(|| {
                Executor::extract_exception(tc_scope)
                    .expect("exception occored but no exception was caught")
            })
            .map_err(ExecutorError::Execute)?;

        // extract main function
        let main_key =
            v8::String::new(tc_scope, "main").wrap_err("failed to create JS string 'main'")?;
        let main_function =
            v8::Local::<v8::Function>::try_from(global.get(tc_scope, main_key.into()).unwrap())
                .map_err(|_e| ExecutorError::NoMain)?;
        let main_function = v8::Global::new(tc_scope, main_function);

        Ok((tc_scope.escape(context), main_function))
    }

    pub fn execute(&mut self, full_text: &str, selection: Option<&str>) -> Result<ExecutionStatus> {
        // setup execution status
        {
            let status_slot = self
                .isolate
                .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
                .wrap_err("failed to get mutable access to status slot")?;

            let mut status = status_slot.borrow_mut();

            status.reset();
            *status.full_text.write() = full_text.to_string();
            status.full_text.clear();
            *status.text.write() = selection.unwrap_or(full_text).to_string();
            status.text.clear();
            *status.selection.write() = selection.unwrap_or("").to_string();
            status.selection.clear();
        }

        // prepare payload and execute main

        // TODO: use ObjectTemplate, problem: rusty_v8 doesn't have set_accessor_with_setter or even set_accessor for
        // object templates
        {
            let state_slot = self
                .isolate
                .get_slot_mut::<Rc<RefCell<ExecutorState>>>()
                .wrap_err("Failed to get mutable access to state slot")?
                .clone();
            let state_slot = state_slot.borrow();

            let context = state_slot
                .global_context
                .as_ref()
                .wrap_err("global_context is not initalizied")?;
            let scope = &mut v8::HandleScope::with_context(&mut self.isolate, context);

            // payload is the object passed into function main
            let payload = v8::Object::new(scope);

            // value: isSelection
            {
                let is_selection_key = v8::String::new(scope, "isSelection")
                    .wrap_err("Failed to construct 'isSelection' JS string")?;

                let is_selection_value = v8::Boolean::new(scope, selection.is_some());

                payload
                    .set(scope, is_selection_key.into(), is_selection_value.into())
                    .wrap_err("Failed to set 'isSelection' value")?;
            }

            // getter/setters: full_text, text, selection
            {
                let full_text_key = v8::String::new(scope, "fullText")
                    .wrap_err("Failed to construct 'fullText' JS string")?;
                let text_key = v8::String::new(scope, "text")
                    .wrap_err("Failed to construct 'text' JS string")?;
                let selection_key = v8::String::new(scope, "selection")
                    .wrap_err("Failed to construct 'selection' JS string")?;

                payload
                    .set_accessor_with_setter(
                        scope,
                        full_text_key.into(),
                        Executor::payload_full_text_getter,
                        Executor::payload_full_text_setter,
                    )
                    .wrap_err("Failed to set 'full_text' accessor")?;
                payload
                    .set_accessor_with_setter(
                        scope,
                        text_key.into(),
                        Executor::payload_text_getter,
                        Executor::payload_text_setter,
                    )
                    .wrap_err("Failed to set 'text' accessor")?;
                payload
                    .set_accessor_with_setter(
                        scope,
                        selection_key.into(),
                        Executor::payload_selection_getter,
                        Executor::payload_selection_setter,
                    )
                    .wrap_err("Failed to set 'selection' accessor")?;
            }

            // functions: post_info, post_error, insert

            let post_info_key = v8::String::new(scope, "postInfo")
                .wrap_err("Failed to create JS string 'postInfo'")?;
            let post_error_key = v8::String::new(scope, "postError")
                .wrap_err("Failed to create JS string 'postError'")?;
            let insert_key =
                v8::String::new(scope, "insert").wrap_err("Failed to create JS string 'insert'")?;

            let post_info_val = v8::Function::new(scope, Executor::payload_post_info)
                .wrap_err("Failed to convert post_info function")?;
            let post_error_val = v8::Function::new(scope, Executor::payload_post_error)
                .wrap_err("Failed to create post_error function")?;
            let insert_val = v8::Function::new(scope, Executor::payload_insert)
                .wrap_err("Failed to create payload_insert function")?;

            payload
                .set(scope, post_info_key.into(), post_info_val.into())
                .wrap_err("Failed to set 'post_info' function")?;
            payload
                .set(scope, post_error_key.into(), post_error_val.into())
                .wrap_err("Failed to set 'post_error' function")?;
            payload
                .set(scope, insert_key.into(), insert_val.into())
                .wrap_err("Failed to set 'insert' function")?;

            let main_function = state_slot
                .main_function
                .as_ref()
                .wrap_err("main_function not initialized")?
                .get(scope);
            let escape_scope = &mut v8::EscapableHandleScope::new(scope);
            let tc_scope = &mut v8::TryCatch::new(escape_scope);

            main_function
                .call(tc_scope, payload.into(), &[payload.into()])
                .ok_or_else(|| {
                    ExecutorError::Execute(
                        Executor::extract_exception(tc_scope)
                            .wrap_err("Exception occored but no exception was caught")
                            .unwrap(),
                    )
                })?;
        }

        // extract execution status
        {
            let status_slot = self
                .isolate
                .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
                .ok_or_else(|| eyre!("Failed to get mutable access to status slot"))?;

            let status = status_slot.borrow();

            Ok(status.clone())
        }
    }

    fn extract_exception(
        tc_scope: &mut v8::TryCatch<v8::EscapableHandleScope>,
    ) -> Result<JSException> {
        let exception_str = tc_scope
            .exception()
            .ok_or_else(|| eyre!("No exception caught"))?
            .to_string(tc_scope)
            .wrap_err("Exception is not a string")?
            .to_rust_string_lossy(tc_scope);

        let message = match tc_scope.message() {
            Some(message) => message,
            None => {
                return Ok(JSException {
                    exception_str,
                    ..Default::default()
                });
            }
        };

        Ok(JSException {
            exception_str,
            resource_name: message
                .get_script_resource_name(tc_scope)
                .and_then(|r| r.to_string(tc_scope))
                .map(|r| r.to_rust_string_lossy(tc_scope)),
            source_line: message
                .get_source_line(tc_scope)
                .map(|l| l.to_rust_string_lossy(tc_scope)),
            line_number: message.get_line_number(tc_scope),
            columns: Some((message.get_start_column(), message.get_end_column())),
        })
    }

    fn global_require(
        scope: &mut v8::HandleScope<'_>,
        args: v8::FunctionCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let code = args
            .get(0)
            .to_string(scope)
            .ok_or_else(|| eyre!("argument to require is not a string"))
            .map(|string_arg| string_arg.to_rust_string_lossy(scope))
            .map(|mut path| {
                if !path.ends_with(".js") {
                    path.push_str(".js");
                }
                info!("loading {}", path);
                path
            })
            // grab the source
            .and_then(Executor::load_raw_source)
            // add boop wrapper
            .map(|raw_source| [BOOP_WRAPPER_START, &raw_source, BOOP_WRAPPER_END].concat())
            // create JS string
            .and_then(|source| {
                v8::String::new(scope, &source)
                    .ok_or_else(|| eyre!("failed to create JS string from source"))
            });

        if let Err(err) = code {
            let exception_str = v8::String::new(scope, &err.to_string())
                .expect("failed to create string for exception");
            let exception = v8::Exception::error(scope, exception_str);

            scope.throw_exception(exception);

            return;
        }

        let code = code.unwrap();

        let export = v8::Script::compile(scope, code, None)
            .ok_or_else(|| eyre!("failed to compile JS"))
            .and_then(|script| {
                script
                    .run(scope)
                    .ok_or_else(|| eyre!("failed to execute JS"))
            });

        match export {
            Ok(export) => rv.set(export),
            Err(err) => error!("failed to require script: {}", err),
        }
    }

    fn payload_post_info(
        scope: &mut v8::HandleScope<'_>,
        args: v8::FunctionCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let info = args
            .get(0)
            .to_string(scope)
            .expect("failed to convert argument to post_info to string")
            .to_rust_string_lossy(scope);

        scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get mutable access to status slot")
            .borrow_mut()
            .info
            .replace(info);

        let undefined = v8::undefined(scope).into();
        rv.set(undefined)
    }

    fn payload_post_error(
        scope: &mut v8::HandleScope<'_>,
        args: v8::FunctionCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let error = args
            .get(0)
            .to_string(scope)
            .expect("failed to convert argument to post_error to string")
            .to_rust_string_lossy(scope);

        scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get mutable access to status slot")
            .borrow_mut()
            .error
            .replace(error);

        let undefined = v8::undefined(scope).into();
        rv.set(undefined)
    }

    fn payload_insert(
        scope: &mut v8::HandleScope<'_>,
        args: v8::FunctionCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let insert = args
            .get(0)
            .to_string(scope)
            .expect("failed to convert insert argument to string")
            .to_rust_string_lossy(scope);

        scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get mutable access to status slot")
            .borrow_mut()
            .insert
            .push(insert);

        let undefined = v8::undefined(scope).into();
        rv.set(undefined)
    }

    fn payload_full_text_getter(
        scope: &mut v8::HandleScope<'_>,
        _key: v8::Local<'_, v8::Name>,
        _args: v8::PropertyCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let full_text = scope
            .get_slot::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get status slot")
            .borrow()
            .full_text
            .read()
            .clone();

        rv.set(
            v8::String::new(scope, &full_text)
                .expect("failed to construct JS string from full_text")
                .into(),
        );
    }

    fn payload_full_text_setter(
        scope: &mut v8::HandleScope<'_>,
        _key: v8::Local<'_, v8::Name>,
        value: v8::Local<'_, v8::Value>,
        _args: v8::PropertyCallbackArguments<'_>,
    ) {
        let new_value = value
            .to_string(scope)
            .expect("failed to convert value to string")
            .to_rust_string_lossy(scope);

        info!("setting full_text ({} bytes)", new_value.len());

        let slot = scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get mutable access to status slot");

        let mut slot = slot.borrow_mut();

        let full_text = slot.full_text.write();

        *full_text = new_value;
    }

    fn payload_text_getter(
        scope: &mut v8::HandleScope<'_>,
        _key: v8::Local<'_, v8::Name>,
        _args: v8::PropertyCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let text = scope
            .get_slot::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get status slot")
            .borrow()
            .text
            .read()
            .clone();

        rv.set(
            v8::String::new(scope, &text)
                .expect("faield to create JS string from text")
                .into(),
        );
    }

    fn payload_text_setter(
        scope: &mut v8::HandleScope<'_>,
        _key: v8::Local<'_, v8::Name>,
        value: v8::Local<'_, v8::Value>,
        _args: v8::PropertyCallbackArguments<'_>,
    ) {
        let new_value = value
            .to_string(scope)
            .expect("failed to convert value to string")
            .to_rust_string_lossy(scope);

        info!("setting text ({} bytes)", new_value.len());

        let slot = scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .expect("faield to get mutable access status slot");

        let mut slot = slot.borrow_mut();

        let text = slot.text.write();

        *text = new_value;
    }

    fn payload_selection_getter(
        scope: &mut v8::HandleScope<'_>,
        _key: v8::Local<'_, v8::Name>,
        _args: v8::PropertyCallbackArguments<'_>,
        mut rv: v8::ReturnValue<'_>,
    ) {
        let selection = scope
            .get_slot::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get status slot")
            .borrow()
            .selection
            .read()
            .clone();

        rv.set(
            v8::String::new(scope, &selection)
                .expect("problem constructing JS string")
                .into(),
        );
    }

    fn payload_selection_setter(
        scope: &mut v8::HandleScope<'_>,
        _key: v8::Local<'_, v8::Name>,
        value: v8::Local<'_, v8::Value>,
        _args: v8::PropertyCallbackArguments<'_>,
    ) {
        let new_value = value
            .to_string(scope)
            .expect("failed to convert value to string")
            .to_rust_string_lossy(scope);

        info!("setting selection ({} bytes)", new_value.len());

        let slot = scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .expect("failed to get mutable access to status slot");

        let mut slot = slot.borrow_mut();

        let selection = slot.selection.write();

        *selection = new_value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate tempfile;
    use std::io::prelude::*;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_error_new_big_string() {
        init();
        let source = "0".repeat(1 << 29);
        let result = Executor::new(&source);
        assert_eq!(
            result.unwrap_err().downcast::<ExecutorError>().unwrap(),
            ExecutorError::SourceExceedsMaxLength
        );
    }

    #[test]
    fn test_error_new_compile() {
        init();
        let source = "this won't compile!";
        let result = Executor::new(&source);
        assert_eq!(
            result.unwrap_err().downcast::<ExecutorError>().unwrap(),
            ExecutorError::Compile(JSException {
                exception_str: "SyntaxError: Unexpected identifier".to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some("this won\'t compile!".to_string()),
                line_number: Some(1),
                columns: Some((5, 8)),
            })
        );
    }

    #[test]
    fn test_error_new_execute() {
        init();
        let source = r#"throw "Woo! Exception!";"#;
        let result = Executor::new(source);
        assert_eq!(
            result.unwrap_err().downcast::<ExecutorError>().unwrap(),
            ExecutorError::Execute(JSException {
                exception_str: "Woo! Exception!".to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some("throw \"Woo! Exception!\";".to_string()),
                line_number: Some(1),
                columns: Some((0, 1))
            })
        );
    }

    #[test]
    fn test_error_execute_no_main() {
        init();
        let source = r#"let i = 100;"#;

        assert_eq!(
            Executor::new(source)
                .unwrap_err()
                .downcast::<ExecutorError>()
                .unwrap(),
            ExecutorError::NoMain
        )
    }

    #[test]
    fn test_error_execute_exception() {
        init();
        let source = r#"function main() {
            throw "(╯°□°）╯︵ ┻━┻";
        }"#;

        assert_eq!(
            Executor::new(source)
                .unwrap()
                .execute("full_text", None)
                .unwrap_err()
                .downcast::<ExecutorError>()
                .unwrap(),
            ExecutorError::Execute(JSException {
                exception_str: "(╯°□°）╯︵ ┻━┻".to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some("            throw \"(╯°□°）╯︵ ┻━┻\";".to_string()),
                line_number: Some(2),
                columns: Some((12, 13))
            })
        );
    }

    #[test]
    fn test_error_require_internal_script() {
        init();
        let source = r#"function main() {
            let foo = require("@boop/non-existant");
        }"#;

        assert_eq!(
            Executor::new(source)
                .unwrap()
                .execute("full_text", None)
                .unwrap_err()
                .downcast::<ExecutorError>()
                .unwrap(),
            ExecutorError::Execute(JSException {
                exception_str: "Error: No internal script with path \"@boop/non-existant.js\""
                    .to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some(
                    "            let foo = require(\"@boop/non-existant\");".to_string()
                ),
                line_number: Some(2),
                columns: Some((22, 23))
            }),
        );
    }

    #[test]
    fn test_error_require_script_missing() {
        init();
        let source = r#"function main() {
            let foo = require("this-script-does-not-exist.js");
        }"#;

        assert_eq!(
            Executor::new(source)
                .unwrap()
                .execute("full_text", None)
                .unwrap_err()
                .downcast::<ExecutorError>()
                .unwrap(),
            ExecutorError::Execute(JSException {
                exception_str: "Error: Could not open \"this-script-does-not-exist.js\""
                    .to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some(
                    "            let foo = require(\"this-script-does-not-exist.js\");".to_string()
                ),
                line_number: Some(2),
                columns: Some((22, 23))
            }),
        );
    }

    #[test]
    fn test_error_require_script_compile_error() {
        init();

        let mut file = tempfile::Builder::new().suffix(".js").tempfile().unwrap();
        write!(file, r#"┻━┻ ︵ ¯\(ツ)/¯ ︵ ┻━┻"#).unwrap();

        let file_name = file.path().file_name().unwrap().to_str().unwrap();

        let source = format!(
            "function main() {{
                let foo = require(\"{}\");
            }}",
            file_name
        );

        assert_eq!(
            Executor::new(&source)
                .unwrap()
                .execute("full_text", None)
                .unwrap_err()
                .downcast::<ExecutorError>()
                .unwrap(),
            ExecutorError::Execute(JSException {
                exception_str: "SyntaxError: Invalid or unexpected token".to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some(r#"┻━┻ ︵ ¯\(ツ)/¯ ︵ ┻━┻"#.to_string()),
                line_number: Some(17),
                columns: Some((0, 0))
            }),
        );
    }

    #[test]
    fn test_error_require_script_execute_error() {
        init();

        let mut file = tempfile::Builder::new().suffix(".js").tempfile().unwrap();
        write!(
            file,
            r#"(function() {{ throw "༼ﾉຈل͜ຈ༽ﾉ︵┻━┻"; return 123 }})()"#
        )
        .unwrap();

        let file_name = file.path().file_name().unwrap().to_str().unwrap();

        let source = format!(
            "function main() {{
                let foo = require(\"{}\");
            }}",
            file_name
        );

        assert_eq!(
            Executor::new(&source)
                .unwrap()
                .execute("full_text", None)
                .unwrap_err()
                .downcast::<ExecutorError>()
                .unwrap(),
            ExecutorError::Execute(JSException {
                exception_str: "༼ﾉຈل\u{35c}ຈ༽ﾉ︵┻━┻".to_string(),
                resource_name: Some("undefined".to_string()),
                source_line: Some(
                    "(function() { throw \"༼ﾉຈل\u{35c}ຈ༽ﾉ︵┻━┻\"; return 123 })()".to_string()
                ),
                line_number: Some(17),
                columns: Some((14, 15))
            })
        )
    }
}

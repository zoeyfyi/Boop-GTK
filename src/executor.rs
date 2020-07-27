use crate::{script::Script, Scripts, PROJECT_DIRS};
use dirty2::Dirty;
use rusty_v8 as v8;
use simple_error::SimpleError;
use std::{cell::RefCell, convert::TryFrom, fs::File, io::Read, rc::Rc};

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

pub struct Executor {
    isolate: Option<v8::OwnedIsolate>,
    script: Script,
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

impl Executor {
    pub fn new(script: Script) -> Self {
        Executor {
            isolate: None,
            script,
        }
    }

    pub fn script(&self) -> &Script {
        &self.script
    }

    // load source code from internal files or external filesystem depending on the path
    fn load_raw_source(path: String) -> Result<String, SimpleError> {
        if path.starts_with("@boop/") {
            // script is internal

            let internal_path = path.replace("@boop/", "lib/");
            info!(
                "found internal script, real path: #BINARY#/{}",
                internal_path
            );

            let raw_source = String::from_utf8(
                Scripts::get(&internal_path)
                    .ok_or_else(|| {
                        SimpleError::new(format!("no internal script with path \"{}\"", path))
                    })?
                    .to_vec(),
            )
            .map_err(|e| SimpleError::with("problem with file encoding", e))?;

            return Ok(raw_source);
        }

        let mut external_path = PROJECT_DIRS.config_dir().to_path_buf();
        external_path.push("scripts");
        external_path.push(&path);

        info!(
            "found external script, real path: {}",
            external_path.display()
        );

        let mut raw_source = String::new();
        File::open(external_path)
            .map_err(|e| SimpleError::with(&format!("could not open \"{}\"", path), e))?
            .read_to_string(&mut raw_source)
            .map_err(|e| SimpleError::with("problem reading file", e))?;

        Ok(raw_source)
    }

    fn initialize_context<'s>(
        script: &Script,
        scope: &mut v8::HandleScope<'s, ()>,
    ) -> (v8::Local<'s, v8::Context>, v8::Global<v8::Function>) {
        let scope = &mut v8::EscapableHandleScope::new(scope);
        let context = v8::Context::new(scope);
        let global = context.global(scope);
        let scope = &mut v8::ContextScope::new(scope, context);

        let require_key = v8::String::new(scope, "require").unwrap();
        let require_val = v8::Function::new(scope, Executor::global_require).unwrap();
        global.set(scope, require_key.into(), require_val.into());

        // complile and run script
        let code = v8::String::new(scope, script.source()).unwrap();
        let compiled_script = v8::Script::compile(scope, code, None).unwrap();
        compiled_script.run(scope).unwrap();

        // extract main function
        let main_key = v8::String::new(scope, "main").unwrap();
        let main_function =
            v8::Local::<v8::Function>::try_from(global.get(scope, main_key.into()).unwrap())
                .unwrap();
        let main_function = v8::Global::new(scope, main_function);

        (scope.escape(context), main_function)
    }

    fn initialize_isolate(&mut self) {
        assert!(self.isolate.is_none());

        info!("initalizing isolate for {}", self.script().metadata().name);

        // set up execution context
        let mut isolate = v8::Isolate::new(Default::default());
        let (global_context, main_function) = {
            let scope = &mut v8::HandleScope::new(&mut isolate);
            // let context = v8::Context::new(scope);
            let (context, main_function) = Executor::initialize_context(&self.script, scope);
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

        self.isolate = Some(isolate);
    }

    pub fn execute(&mut self, full_text: &str, selection: Option<&str>) -> ExecutionStatus {
        if self.isolate.is_none() {
            self.initialize_isolate();
        }

        // setup execution status
        {
            let isolate = self.isolate.as_ref().unwrap();

            let status_slot = isolate
                .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
                .unwrap();

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
            let isolate = self.isolate.as_mut().unwrap();

            let state_slot = isolate
                .get_slot_mut::<Rc<RefCell<ExecutorState>>>()
                .unwrap()
                .clone();
            let state_slot = state_slot.borrow();

            let context = state_slot.global_context.as_ref().unwrap();
            let scope = &mut v8::HandleScope::with_context(isolate, context);

            // payload is the object passed into function main
            let payload = v8::Object::new(scope);

            // getter/setters: full_text, text, selection
            {
                let full_text_key = v8::String::new(scope, "fullText").unwrap();
                let text_key = v8::String::new(scope, "text").unwrap();
                let selection_key = v8::String::new(scope, "selection").unwrap();

                payload.set_accessor_with_setter(
                    scope,
                    full_text_key.into(),
                    Executor::payload_full_text_getter,
                    Executor::payload_full_text_setter,
                );
                payload.set_accessor_with_setter(
                    scope,
                    text_key.into(),
                    Executor::payload_text_getter,
                    Executor::payload_text_setter,
                );
                payload.set_accessor_with_setter(
                    scope,
                    selection_key.into(),
                    Executor::payload_selection_getter,
                    Executor::payload_selection_setter,
                );
            }

            // functions: post_info, post_error, insert
            {
                let post_info_key = v8::String::new(scope, "postInfo").unwrap();
                let post_error_key = v8::String::new(scope, "postError").unwrap();
                let insert_key = v8::String::new(scope, "insert").unwrap();

                let post_info_val = v8::Function::new(scope, Executor::payload_post_info).unwrap();
                let post_error_val =
                    v8::Function::new(scope, Executor::payload_post_error).unwrap();
                let insert_val = v8::Function::new(scope, Executor::payload_insert).unwrap();

                payload.set(scope, post_info_key.into(), post_info_val.into());
                payload.set(scope, post_error_key.into(), post_error_val.into());
                payload.set(scope, insert_key.into(), insert_val.into());
            }

            state_slot.main_function.as_ref().unwrap().get(scope).call(
                scope,
                payload.into(),
                &[payload.into()],
            );
        }

        // extract execution status
        {
            let status_slot = self
                .isolate
                .as_ref()
                .unwrap()
                .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
                .unwrap();

            let status = (status_slot).borrow();

            status.clone()
        }
    }

    fn global_require(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let mut path = args
            .get(0)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        info!("loading {}", path);

        // append extension
        if !path.ends_with(".js") {
            path.push_str(".js");
        }

        match Executor::load_raw_source(path) {
            Ok(raw_source) => {
                let source = format!("{}{}{}", BOOP_WRAPPER_START, raw_source, BOOP_WRAPPER_END);

                let code = v8::String::new(scope, &source).unwrap();
                let compiled_script = v8::Script::compile(scope, code, None).unwrap();
                let export = compiled_script.run(scope).unwrap();

                rv.set(export);
            }
            Err(e) => {
                warn!("problem requiring script, {}", e);

                let undefined = v8::undefined(scope).into();
                rv.set(undefined)
            }
        }
    }

    fn payload_post_info(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let info = args
            .get(0)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap()
            .borrow_mut()
            .info
            .replace(info);

        let undefined = v8::undefined(scope).into();
        rv.set(undefined)
    }

    fn payload_post_error(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let error = args
            .get(0)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap()
            .borrow_mut()
            .error
            .replace(error);

        let undefined = v8::undefined(scope).into();
        rv.set(undefined)
    }

    fn payload_insert(
        scope: &mut v8::HandleScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let insert = args
            .get(0)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap()
            .borrow_mut()
            .insert
            .push(insert);

        let undefined = v8::undefined(scope).into();
        rv.set(undefined)
    }

    fn payload_full_text_getter(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        _args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let full_text = scope
            .get_slot::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap()
            .borrow()
            .full_text
            .read()
            .clone();

        rv.set(v8::String::new(scope, &full_text).unwrap().into());
    }

    fn payload_full_text_setter(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        value: v8::Local<v8::Value>,
        _args: v8::PropertyCallbackArguments,
    ) {
        let new_value = value.to_string(scope).unwrap().to_rust_string_lossy(scope);

        info!("setting full_text ({} bytes)", new_value.len());

        let slot = scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap();

        let mut slot = slot.borrow_mut();

        let full_text = slot.full_text.write();

        *full_text = new_value;
    }

    fn payload_text_getter(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        _args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let text = scope
            .get_slot::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap()
            .borrow()
            .text
            .read()
            .clone();

        rv.set(v8::String::new(scope, &text).unwrap().into());
    }

    fn payload_text_setter(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        value: v8::Local<v8::Value>,
        _args: v8::PropertyCallbackArguments,
    ) {
        let new_value = value.to_string(scope).unwrap().to_rust_string_lossy(scope);

        info!("setting text ({} bytes)", new_value.len());

        let slot = scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap();

        let mut slot = slot.borrow_mut();

        let text = slot.text.write();

        *text = new_value;
    }

    fn payload_selection_getter(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        _args: v8::PropertyCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let selection = scope
            .get_slot::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap()
            .borrow()
            .selection
            .read()
            .clone();

        rv.set(v8::String::new(scope, &selection).unwrap().into());
    }

    fn payload_selection_setter(
        scope: &mut v8::HandleScope,
        _key: v8::Local<v8::Name>,
        value: v8::Local<v8::Value>,
        _args: v8::PropertyCallbackArguments,
    ) {
        let new_value = value.to_string(scope).unwrap().to_rust_string_lossy(scope);

        info!("setting selection ({} bytes)", new_value.len());

        let slot = scope
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap();

        let mut slot = slot.borrow_mut();

        let selection = slot.selection.write();

        *selection = new_value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ParseScriptError;
    use std::{borrow::Cow, sync::Mutex};

    lazy_static! {
        static ref INIT_LOCK: Mutex<u32> = Mutex::new(0);
    }

    #[must_use]
    struct SetupGuard {}

    fn setup() -> SetupGuard {
        let mut g = INIT_LOCK.lock().unwrap();
        *g += 1;
        if *g == 1 {
            v8::V8::initialize_platform(v8::new_default_platform().unwrap());
            v8::V8::initialize();
        }
        SetupGuard {}
    }

    #[test]
    fn test_retain_execution_context() {
        let _guard = setup();

        let mut executor = Executor::new(
            Script::from_source(
                "
            /**
                {
                    \"api\":1,
                    \"name\":\"Counter\",
                    \"description\":\"Counts up\",
                    \"author\":\"Ben\",
                    \"icon\":\"html\",
                    \"tags\":\"count\"
                }
            **/
            
            let number = 0;
            
            function main(state) {
                number += 1;
                state.text = number;
            }"
                .to_string(),
            )
            .unwrap(),
        );

        for i in 1..10 {
            let status = executor.execute("", None);
            assert_eq!(
                TextReplacement::Full(i.to_string()),
                status.into_replacement()
            );
        }
    }

    #[test]
    fn test_builtin_scripts() {
        let _guard = setup();

        use rust_embed::RustEmbed;

        #[derive(RustEmbed)]
        #[folder = "submodules/Boop/Boop/Boop/scripts/"]
        struct Scripts;

        for file in Scripts::iter() {
            println!("testing {}", file);

            let source: Cow<'static, [u8]> = Scripts::get(&file).unwrap();
            let script_source = String::from_utf8(source.to_vec()).unwrap();

            match Script::from_source(script_source) {
                Ok(script) => {
                    let mut executor = Executor::new(script);
                    executor.execute(
                        "foobar ♈ ♉ ♊ ♋ ♌ ♍ ♎ ♏ ♐ ♑ ♒ ♓ 😁 😝 😋 😄",
                        None,
                    );
                }
                Err(e) => match e {
                    ParseScriptError::NoMetadata => {
                        assert!(file.starts_with("lib/")); // only library files should fail
                    }
                    ParseScriptError::InvalidMetadata(_) => assert!(false),
                },
            }
        }
    }
}

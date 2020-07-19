use crate::{script::Script, Scripts, PROJECT_DIRS};
use dirty::Dirty;
use rusty_v8 as v8;
use simple_error::SimpleError;
use std::{cell::RefCell, fs::File, io::Read, ptr, rc::Rc};

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
    // v8
    is_v8_initalized: bool,
    isolate: *mut v8::OwnedIsolate,
    handle_scope: *mut v8::HandleScope<'static, ()>,
    context: *mut v8::Local<'static, v8::Context>,
    scope: *mut v8::ContextScope<'static, v8::HandleScope<'static, v8::Context>>,

    // script
    script: Script,
    main_function: *mut v8::Local<'static, v8::Function>,
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
            is_v8_initalized: false,
            isolate: ptr::null_mut(),
            handle_scope: ptr::null_mut(),
            context: ptr::null_mut(),
            scope: ptr::null_mut(),
            script,
            main_function: ptr::null_mut(),
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

    unsafe fn initalize_v8(&mut self) {
        assert!(!self.is_v8_initalized);
        assert!(self.isolate.is_null());
        assert!(self.handle_scope.is_null());
        assert!(self.context.is_null());
        assert!(self.scope.is_null());

        info!("initalizing isolate for {}", self.script().metadata().name);

        // set up execution context
        self.isolate = Box::into_raw(Box::new(v8::Isolate::new(Default::default())));
        self.handle_scope = Box::into_raw(Box::new(v8::HandleScope::new(&mut *self.isolate)));
        self.context = Box::into_raw(Box::new(v8::Context::new(&mut *self.handle_scope)));
        self.scope = Box::into_raw(Box::new(v8::ContextScope::new(
            &mut *self.handle_scope,
            *self.context,
        )));

        let status_slot: Rc<RefCell<ExecutionStatus>> =
            Rc::new(RefCell::new(ExecutionStatus::default()));
        self.isolate.as_mut().unwrap().set_slot(status_slot);

        // add require to global scope
        let require = v8::Function::new(&mut *self.scope, Executor::global_require).unwrap();

        let key_require = v8::String::new(&mut *self.scope, "require").unwrap();

        (&*self.context).global(&mut *self.scope).set(
            &mut *self.scope,
            key_require.into(),
            require.into(),
        );

        // complile and run script
        let code = v8::String::new(&mut *self.scope, self.script.source()).unwrap();
        let compiled_script = v8::Script::compile(&mut *self.scope, code, None).unwrap();
        compiled_script.run(&mut *self.scope).unwrap();

        // extract main function
        let function_name = v8::String::new(&mut *self.scope, "main").unwrap();
        self.main_function = {
            Box::into_raw(Box::new(v8::Local::cast(
                (*self.context)
                    .global(&mut *self.scope)
                    .get(&mut *self.scope, function_name.into())
                    .unwrap(),
            )))
        };

        self.is_v8_initalized = true;
    }

    unsafe fn internal_execute(
        &mut self,
        full_text: &str,
        selection: Option<&str>,
    ) -> ExecutionStatus {
        if !self.is_v8_initalized {
            self.initalize_v8();
        }

        // setup execution status
        {
            let isolate = self.isolate.as_ref().unwrap();

            let slot = isolate
                .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
                .unwrap();

            let mut status = slot.borrow_mut();

            status.reset();
            *status.full_text.write() = full_text.to_string();
            status.full_text.clear();
            *status.text.write() = selection.unwrap_or(full_text).to_string();
            status.text.clear();
            *status.selection.write() = selection.unwrap_or("").to_string();
            status.selection.clear();
        }

        // prepare payload
        // TODO: use ObjectTemplate, problem: rusty_v8 doesn't have set_accessor_with_setter or even set_accessor for
        // object templates
        let payload = v8::Object::new(&mut *self.scope);

        let key_full_text = v8::String::new(&mut *self.scope, "fullText").unwrap();
        let key_text = v8::String::new(&mut *self.scope, "text").unwrap();
        let key_selection = v8::String::new(&mut *self.scope, "selection").unwrap();
        let key_post_info = v8::String::new(&mut *self.scope, "postInfo").unwrap();
        let key_post_error = v8::String::new(&mut *self.scope, "postError").unwrap();
        let key_insert = v8::String::new(&mut *self.scope, "insert").unwrap();

        let post_info = v8::Function::new(&mut *self.scope, Executor::payload_post_info).unwrap();
        let post_error = v8::Function::new(&mut *self.scope, Executor::payload_post_error).unwrap();
        let insert = v8::Function::new(&mut *self.scope, Executor::payload_insert).unwrap();

        payload.set_accessor_with_setter(
            &mut *self.scope,
            key_full_text.into(),
            Executor::payload_full_text_getter,
            Executor::payload_full_text_setter,
        );
        payload.set_accessor_with_setter(
            &mut *self.scope,
            key_text.into(),
            Executor::payload_text_getter,
            Executor::payload_text_setter,
        );
        payload.set_accessor_with_setter(
            &mut *self.scope,
            key_selection.into(),
            Executor::payload_selection_getter,
            Executor::payload_selection_setter,
        );

        payload.set(&mut *self.scope, key_post_info.into(), post_info.into());
        payload.set(&mut *self.scope, key_post_error.into(), post_error.into());
        payload.set(&mut *self.scope, key_insert.into(), insert.into());

        // call main
        { &mut *self.main_function }.call(&mut *self.scope, payload.into(), &[payload.into()]);

        let status_slot = self
            .isolate
            .as_ref()
            .unwrap()
            .get_slot_mut::<Rc<RefCell<ExecutionStatus>>>()
            .unwrap();

        let status = (status_slot).borrow();

        status.clone()
    }

    pub fn execute(&mut self, full_text: &str, selection: Option<&str>) -> ExecutionStatus {
        unsafe { self.internal_execute(full_text, selection) }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        if !self.is_v8_initalized {
            return;
        }

        unsafe {
            Box::from_raw(self.scope);
            Box::from_raw(self.context);
            Box::from_raw(self.handle_scope);
            Box::from_raw(self.isolate);
        }
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

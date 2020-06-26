extern crate rusty_v8;

use crate::script::Script;
use rusty_v8 as v8;
use std::sync::RwLock;

pub struct Executor<'a> {
    script: &'a Script,
}

pub struct ExecutionResult {
    pub replacement: TextReplacement,
    pub info: Option<String>,
    pub error: Option<String>,
}
pub enum TextReplacement {
    Full(String),
    Selection(String),
}

impl<'a> Executor<'a> {
    pub fn new(script: &'a Script) -> Self {
        Executor { script }
    }

    pub fn execute(self, full_text: &str, selection: Option<&str>) -> ExecutionResult {
        // setup instance of v8
        let mut isolate = v8::Isolate::new(Default::default());
        let mut handle_scope = v8::HandleScope::new(&mut isolate);
        let scope = handle_scope.enter();
        let context: v8::Local<v8::Context> = v8::Context::new(scope);
        let mut context_scope = v8::ContextScope::new(scope, context);
        let scope = context_scope.enter();

        // complile and run script
        let code = v8::String::new(scope, self.script.source()).unwrap();
        let mut compiled_script = v8::Script::compile(scope, context, code, None).unwrap();
        compiled_script.run(scope, context).unwrap();

        // extract main function
        let function_name = v8::String::new(scope, "main").unwrap();
        let function: v8::Local<v8::Function> = unsafe {
            v8::Local::cast(
                context
                    .global(scope)
                    .get(scope, context, function_name.into())
                    .unwrap(),
            )
        };

        lazy_static! {
            static ref INFO: RwLock<Option<String>> = RwLock::new(None);
            static ref ERROR: RwLock<Option<String>> = RwLock::new(None);
        }

        // reset info/error
        {
            let mut info = INFO.write().unwrap();
            *info = None;
            let mut error = ERROR.write().unwrap();
            *error = None;
        }

        // create postInfo and postError functions
        let post_info = v8::Function::new(
            scope,
            context,
            |scope: v8::FunctionCallbackScope,
             args: v8::FunctionCallbackArguments,
             mut rv: v8::ReturnValue| {
                let mut i = INFO.write().unwrap();
                *i = Some(
                    args.get(0)
                        .to_string(scope)
                        .unwrap()
                        .to_rust_string_lossy(scope),
                );
                // test.fetch_add(10, Relaxed);
                rv.set(v8::undefined(scope).into())
            },
        )
        .unwrap();

        let post_error = v8::Function::new(
            scope,
            context,
            |scope: v8::FunctionCallbackScope,
             args: v8::FunctionCallbackArguments,
             mut rv: v8::ReturnValue| {
                let mut i = ERROR.write().unwrap();
                *i = Some(
                    args.get(0)
                        .to_string(scope)
                        .unwrap()
                        .to_rust_string_lossy(scope),
                );
                // test.fetch_add(10, Relaxed);
                rv.set(v8::undefined(scope).into())
            },
        )
        .unwrap();

        // prepare payload
        let payload = v8::Object::new(scope);

        // fullText

        let key_full_text = v8::String::new(scope, "fullText").unwrap();
        payload.set(
            context,
            key_full_text.into(),
            v8::String::new(scope, full_text).unwrap().into(),
        );

        // text
        let key_text = v8::String::new(scope, "text").unwrap();
        payload.set(
            context,
            key_text.into(),
            v8::String::new(scope, selection.unwrap_or(full_text))
                .unwrap()
                .into(),
        );

        // selection
        let key_selection = v8::String::new(scope, "selection").unwrap();
        payload.set(
            context,
            key_selection.into(),
            v8::String::new(scope, selection.unwrap_or(""))
                .unwrap()
                .into(),
        );

        // postInfo
        let key_post_info = v8::String::new(scope, "postInfo").unwrap();
        payload.set(context, key_post_info.into(), post_info.into());

        // postError
        let key_post_error = v8::String::new(scope, "postError").unwrap();
        payload.set(context, key_post_error.into(), post_error.into());

        // call main
        function.call(scope, context, payload.into(), &[payload.into()]);

        // extract result
        // TODO(mrbenshef): it would be better to use accessors/interseptors, so we don't have to
        // compare potentially very large strings. however, I can't figure out how to do this
        // without static RwLock's
        let new_text_value = payload
            .get(scope, context, key_text.into())
            .unwrap()
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);
        let new_full_text_value = payload
            .get(scope, context, key_full_text.into())
            .unwrap()
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);
        let new_selection_value = payload
            .get(scope, context, key_selection.into())
            .unwrap()
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        // not quite sure what the correct behaviour here should be
        // right now the order of presidence is:
        // 1. fullText
        // 2. selection
        // 3. text (with select)
        // 4. text (without selection)
        let replacement = if new_full_text_value != full_text {
            info!("found full_text replacement");
            TextReplacement::Full(new_full_text_value)
        } else if selection.is_some() && new_selection_value != selection.unwrap() {
            info!("found selection replacement");
            TextReplacement::Selection(new_selection_value)
        } else if selection.is_some() {
            info!("found text (with selection) replacement");
            TextReplacement::Selection(new_text_value)
        } else {
            info!("found text (without selection) replacement");
            TextReplacement::Full(new_text_value)
        };

        ExecutionResult {
            replacement,
            info: INFO.read().unwrap().clone(),
            error: ERROR.read().unwrap().clone(),
        }
    }
}

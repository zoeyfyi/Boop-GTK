extern crate rusty_v8;

use rusty_v8 as v8;
use std::sync::RwLock;

pub struct Executor;

#[derive(Default)]
pub struct ExecutionResult {
    pub text: String,
    pub info: Option<String>,
    pub error: Option<String>,
}

impl Executor {
    pub fn execute(source: &str, text: &str) -> ExecutionResult {
        // setup instance of v8
        let mut isolate = v8::Isolate::new(Default::default());
        let mut handle_scope = v8::HandleScope::new(&mut isolate);
        let scope = handle_scope.enter();
        let context: v8::Local<v8::Context> = v8::Context::new(scope);
        let mut context_scope = v8::ContextScope::new(scope, context);
        let scope = context_scope.enter();

        // complile and run script
        let code = v8::String::new(scope, source).unwrap();
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
        ).unwrap();

        // prepare payload
        let payload = v8::Object::new(scope);

        let key_text = v8::String::new(scope, "text").unwrap();
        payload.set(
            context,
            key_text.into(),
            v8::String::new(scope, text).unwrap().into(),
        );

        let key_post_info = v8::String::new(scope, "postInfo").unwrap();
        payload.set(context, key_post_info.into(), post_info.into());

        let key_post_error = v8::String::new(scope, "postError").unwrap();
        payload.set(context, key_post_error.into(), post_error.into());

        // call main
        function.call(scope, context, payload.into(), &[payload.into()]);

        // extract result
        let new_text_value = payload.get(scope, context, key_text.into()).unwrap();
        let new_text = new_text_value
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        ExecutionResult {
            text: new_text,
            info: INFO.read().unwrap().clone(),
            error: ERROR.read().unwrap().clone(),
        }
    }
}

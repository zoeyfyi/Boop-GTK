extern crate rusty_v8;

use rusty_v8 as v8;

pub struct Executor;

impl Executor {
    pub fn execute(source: &str, text: &str) -> String {
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

        // prepare payload
        let payload = v8::Object::new(scope);
        let key_text = v8::String::new(scope, "text").unwrap();
        payload.set(
            context,
            key_text.into(),
            v8::String::new(scope, text).unwrap().into(),
        );

        // call main
        function.call(scope, context, payload.into(), &[payload.into()]);

        // extract result
        let new_text_value = payload.get(scope, context, key_text.into()).unwrap();
        let new_text = new_text_value
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);

        return new_text;
    }
}

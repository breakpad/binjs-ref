// FIXME: Too much copy&paste in this file.

use ast::grammar::{ ASTError, FieldName, Kind, Syntax };
use ast::library::{ BINJS_CONST_NAME, BINJS_LET_NAME, BINJS_VAR_NAME, BINJS_DIRECT_EVAL, BINJS_CAPTURED_NAME, SCOPE_NAME };
use util::{ Dispose, JSONGetter };

use serde_json;
use serde_json::Value as JSON;

use std;
use std::cell::{ RefCell, Ref };
use std::collections::HashSet;
use std::rc::{ Rc, Weak };

type Object = serde_json::Map<String, JSON>;

/// The position currently being examined.
#[derive(Clone)]
pub struct Position {
    /// The kind of object currently being examined.
    kind: Kind,

    /// If a field is being examined, its name.
    field: Option<FieldName>
}
impl Position {
    pub fn new(kind: &Kind, field: Option<&FieldName>) -> Self {
        Position {
            kind: kind.clone(),
            field: field.map(FieldName::clone)
        }
    }
    pub fn field(&self) -> &Option<FieldName> {
        &self.field
    }

    /// Get the name of the current field, if any.
    pub fn field_str(&self) -> Option<&str> {
        match self.field {
            None => None,
            Some(ref x) => Some(x.to_str())
        }
    }
    pub fn kind(&self) -> &Kind {
        &self.kind
    }
}

/// Storage for information collected while annotating an AST.
pub struct Context<'a, T> where ContextContents<'a, T>: Dispose {
    /// The current position.
    ///
    /// Copied here to avoid some issues with borrowing. Immutable.
    position: Position,

    /// A shared pointer towards the information collected so far.
    ///
    /// This pointer is shared with children whenever we `enter_field`
    /// or `enter_obj` a new
    contents: Rc<RefCell<ContextContents<'a, T>>>,
}

impl<'a, T> Drop for Context<'a, T> where ContextContents<'a, T>: Dispose {
    fn drop(&mut self) {
        self.contents.borrow_mut().dispose()
    }
}

impl<'a, T> Context<'a, T> where T: Default, ContextContents<'a, T>: Dispose {
    pub fn new(syntax: &'a Syntax) -> Self {
        let contents = ContextContents::new(syntax);
        let position = contents.position.clone();
        let mut result = Context {
            position,
            contents: Rc::new(RefCell::new(contents)),
        };
        result.use_as_lex_scope();
        result.use_as_var_scope();
        result
    }
    pub fn lex_scope(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        self.contents.borrow().lex_scope()
    }
    pub fn fun_scope(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        self.contents.borrow().fun_scope()
    }
    pub fn var_scope(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        self.contents.borrow().var_scope()
    }
    pub fn use_as_fun_scope(&mut self) {
        let weak = Rc::downgrade(&self.contents);
        let mut borrow = self.contents.borrow_mut();
        borrow.fun_scope = Some(weak);
        borrow.is_function_toplevel = true;
    }
    pub fn use_as_lex_scope(&mut self) {
        let weak = Rc::downgrade(&self.contents);
        self.contents
            .borrow_mut()
            .lex_scope = Some(weak);
    }
    pub fn use_as_var_scope(&mut self) {
        let weak = Rc::downgrade(&self.contents);
        self.contents
            .borrow_mut()
            .var_scope = Some(weak);
    }
    pub fn enter_field(&mut self, field: &str) -> Result<Self, ASTError> {
        let position = {
            let grammar = self.contents.borrow()
                .grammar;
            let ref kind = self.contents.borrow().position.kind;
            let field = grammar.get_field_name(field)
                    .ok_or_else(|| ASTError::InvalidField(field.to_string()))?;
            Position::new(kind, Some(field))
        };
        let lex_scope = self.contents.borrow().lex_scope.clone();
        let fun_scope = self.contents.borrow().fun_scope.clone();
        let var_scope = self.contents.borrow().var_scope.clone();
        Ok(Context {
            position: position.clone(),
            contents: Rc::new(RefCell::new(
                ContextContents {
                    position,
                    lex_scope,
                    var_scope,
                    fun_scope,
                    parent: Some(self.contents.clone()),
                    ..ContextContents::new(self.contents.borrow().grammar)
                }
            )),
        })
    }
    pub fn enter_obj(&mut self, kind: &str) -> Result<Self, ASTError> {
        let position = {
            let grammar = self.contents.borrow()
                .grammar;
            let kind = grammar.get_kind(kind)
                .ok_or_else(|| ASTError::InvalidKind(kind.to_string()))?;
            Position::new(&kind, None)
        };
        let lex_scope = self.contents.borrow().lex_scope.clone();
        let fun_scope = self.contents.borrow().fun_scope.clone();
        let var_scope = self.contents.borrow().var_scope.clone();
        Ok(Context {
            position: position.clone(),
            contents: Rc::new(RefCell::new(
                ContextContents {
                    position,
                    fun_scope,
                    lex_scope,
                    var_scope,
                    parent: Some(self.contents.clone()),
                    ..ContextContents::new(self.contents.borrow().grammar)
                }
            )),
        })
    }
    pub fn kind_str(&self) -> &str {
        self.position.kind().to_str()
    }
    pub fn contents(&self) -> Ref<ContextContents<'a, T>> {
        self.contents.borrow()
    }
    pub fn parent(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        self.contents.borrow().parent()
    }
}

pub struct ContextContents<'a, T> {
    data: T,
    position: Position,
    grammar: &'a Syntax,
    parent: Option<Rc<RefCell<ContextContents<'a, T>>>>,
    is_function_toplevel: bool,
    lex_scope: Option<Weak<RefCell<ContextContents<'a, T>>>>,
    var_scope: Option<Weak<RefCell<ContextContents<'a, T>>>>,
    fun_scope: Option<Weak<RefCell<ContextContents<'a, T>>>>,
}

impl<'a, T> ContextContents<'a, T> where T: Default {
    /// Create a new ContextContents containing no data, no parent,
    /// at the default position.
    fn new(syntax: &'a Syntax) -> Self {
        let root = syntax.get_root()
            .kind()
            .expect("Could not extract kind of syntax root");
        ContextContents {
            grammar: syntax,
            position: Position::new(&root, None),
            parent: None,
            lex_scope: None,
            var_scope: None,
            fun_scope: None,
            is_function_toplevel: false,
            data: Default::default()
        }
    }
}
impl<'a, T> ContextContents<'a, T> {
    pub fn field_str(&self) -> Option<&str> {
        self.position.field_str()
    }
    pub fn kind_str(&self) -> &str {
        self.position.kind.to_str()
    }
    pub fn parent(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        if let Some(ref p) = self.parent {
            let borrow = p.borrow();
            if borrow.field_str().is_none() {
                return borrow.parent()
            }
        }
        return self.parent.clone()
    }
    pub fn fun_scope(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        match self.fun_scope {
            None => None,
            Some(ref scope) => Some(Weak::upgrade(scope).expect("fun_scope has been garbage-collected"))
        }
    }
    pub fn var_scope(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        match self.var_scope {
            None => None,
            Some(ref scope) => Some(Weak::upgrade(scope).expect("var_scope has been garbage-collected"))
        }
    }
    pub fn lex_scope(&self) -> Option<Rc<RefCell<ContextContents<'a, T>>>> {
        match self.lex_scope {
            None => None,
            Some(ref scope) => Some(Weak::upgrade(scope).expect("lex_scope has been garbage-collected"))
        }
    }
}

/// The contents of a context used to annotate use of variables.
#[derive(Default)]
pub struct RefContents {
    /// Bound names. Not propagated upwards.
    binding: HashSet<String>,

    /// Free names. Converted to free_in_nested_function when crossing function boundaries.
    free:  HashSet<String>,

    /// Free names captured by a nested function.
    free_in_nested_function: HashSet<String>,

    /// `true` if we have detected a call `eval("foo")` anywhere in the subtree
    /// (including subfunctions), where `eval` is a free name.
    has_direct_eval: bool,
}


impl<'a> ContextContents<'a, RefContents> {
    pub fn add_free_name(&mut self, name: &str) {
        self.data.free.insert(name.to_string());
    }
    pub fn add_binding(&mut self, name: &str) {
        self.data.binding.insert(name.to_string());
    }
    pub fn add_direct_eval(&mut self) {
        self.data.has_direct_eval = true;
    }

    pub fn is_bound(&self, name: &str) -> bool {
        if self.data.binding.contains(name) {
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().is_bound(name);
        }
        false
    }

    pub fn bindings(&self) -> &HashSet<String> {
        &self.data.binding
    }
}

impl<'a> Dispose for ContextContents<'a, RefContents> {
    fn dispose(&mut self) {
        if let Some(ref mut parent) = self.parent {
            let mut parent = parent.borrow_mut();
            parent.data.has_direct_eval |= self.data.has_direct_eval;

            // Drop `bind`.
            let mut binding = HashSet::new();
            std::mem::swap(&mut self.data.binding, &mut binding); // Swap to avoid borrow issues.

            if self.is_function_toplevel {
                // Merge `free` into `free_in_nested_function`...
                for free in self.data.free.drain() {
                    self.data.free_in_nested_function.insert(free);
                }
                self.data.free = HashSet::new();
            }
            // Keep `free`, `free_in_nested_function`... unless the name is bound

            for free in self.data.free.drain() {
                if !binding.contains(&free) {
                    println!("DEBUG: Free variable {} is still free", free);
                    parent.data.free.insert(free);
                }
            }
            for free in self.data.free_in_nested_function.drain() {
                if !binding.contains(&free) {
                    println!("DEBUG: Free-in-nested variable {} is still free", free);
                    parent.data.free_in_nested_function.insert(free);
                }
            }

            println!("DEBUG: Propagating free {:?}", parent.data.free);
            println!("DEBUG: Propagating free_in_nested_function {:?}", parent.data.free_in_nested_function);
        }
    }
}




impl<'a> Context<'a, RefContents> {
    fn captured_names(&self) -> Vec<String> {
        let mut captured_names = vec![];

        let borrow = self.contents.borrow();
        for name in &borrow.data.free_in_nested_function {
            if borrow.data.binding.contains(name) {
                captured_names.push(name.clone());
            }
        }
        captured_names
    }
    pub fn add_direct_eval(&mut self) {
        self.contents.borrow_mut().data.has_direct_eval = true;
    }
    pub fn add_free_name(&mut self, name: &str) {
        self.contents.borrow_mut().data.free.insert(name.to_string());
    }
    pub fn add_binding(&mut self, name: &str) {
        self.contents.borrow_mut().add_binding(name);
    }

    /// Check whether the name is bound somewhere on the stack.
    pub fn is_bound(&self, name: &str) -> bool {
        let borrow = self.contents.borrow();
        borrow.is_bound(name)
    }

    pub fn store(&self, parent: &mut Object) {
        let mut object = parent.get_object_mut(SCOPE_NAME, "Scope field")
            .expect("Could not store captured names, etc.");
        for name in &[BINJS_VAR_NAME, BINJS_LET_NAME, BINJS_CONST_NAME] {
            let name = name.to_string();
            if !object.contains_key(&name) {
                object.insert(name, json!([]));
            }
        }

        assert!(!object.contains_key(BINJS_CAPTURED_NAME));
        assert!(!object.contains_key(BINJS_DIRECT_EVAL));

        let mut captured_names = self.captured_names();
        captured_names.sort();
        let borrow = self.contents.borrow();
        assert!(object
            .insert(BINJS_CAPTURED_NAME.to_string(), json!(captured_names))
            .is_none(),
            "This node already has a field {}", BINJS_CAPTURED_NAME);

        assert!(object
            .insert(BINJS_DIRECT_EVAL.to_string(), json!(borrow.data.has_direct_eval))
            .is_none(),
            "This node already has a field {}", BINJS_DIRECT_EVAL);
    }
    pub fn load(&mut self, parent: &Object) {
        let object = parent.get_object(SCOPE_NAME, "Scope field")
            .expect("Could not load previous pass.");

        let mut borrow = self.contents.borrow_mut();

        let var_decl_names = object.get_array(BINJS_VAR_NAME, "Repository of VarDecl bindings")
            .expect("Could not fetch VarDecl repository");
        for item in var_decl_names {
            let item = item.as_str()
                .expect("Item should be a string")
                .to_string();
            borrow.data.binding.insert(item);
        }

        let let_names = object.get_array(BINJS_LET_NAME, "Repository of LexDecl bindings")
            .expect("Could not fetch LexDecl repository");
        for item in let_names {
            let item = item.as_str()
                .expect("Item should be a string")
                .to_string();
            borrow.data.binding.insert(item);
        }

        let const_names = object.get_array(BINJS_CONST_NAME, "Repository of LexDecl bindings")
            .expect("Could not fetch LexDecl repository");
        for item in const_names {
            let item = item.as_str()
                .expect("Item should be a string")
                .to_string();
            borrow.data.binding.insert(item);
        }
    }
}
#[derive(Clone)]
pub enum DeclPosition {
    Function,
    FunctionArguments,
    Block,
    VarDecl,
    LexDecl,
    Expression,
    Callee,
    Other
}
impl Default for DeclPosition {
    fn default() -> Self {
        DeclPosition::other("uninitialized")
    }
}
impl DeclPosition {
    pub fn other(_: &str) -> Self {
        DeclPosition::Other
    }
}
#[derive(Default)]
pub struct DeclContents {
    const_names: HashSet<String>,
    let_names: HashSet<String>,
    var_names: HashSet<String>,
    scope_kind: ScopeKind,
    uses_strict: bool,
}

#[derive(Clone, Copy)]
pub enum ScopeKind {
    VarDecl,
    LetDecl,
    ConstDecl,
    Nothing
}
impl Default for ScopeKind {
    fn default() -> Self {
        ScopeKind::Nothing
    }
}
impl<'a> ContextContents<'a, DeclContents> {
    pub fn uses_strict(&self) -> bool {
        if self.data.uses_strict {
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().uses_strict();
        }
        false
    }
    pub fn set_uses_strict(&mut self, value: bool) {
        self.data.uses_strict = value
    }
    pub fn is_lex_bound(&self, name: &str) -> bool {
        if self.data.let_names.contains(name) {
            return true;
        }
        if self.data.const_names.contains(name) {
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().is_lex_bound(name);
        }
        false
    }
    pub fn scope_kind(&self) -> ScopeKind {
        if let ScopeKind::Nothing = self.data.scope_kind {
            if let Some(ref parent) = self.parent {
                return parent.borrow().scope_kind()
            }
            return ScopeKind::Nothing
        }
        return self.data.scope_kind
    }
    pub fn add_var_name(&mut self, name: &str) {
        self.data.var_names.insert(name.to_string());
    }
    pub fn add_let_name(&mut self, name: &str) {
        self.data.let_names.insert(name.to_string());
    }
    pub fn add_const_name(&mut self, name: &str) {
        self.data.const_names.insert(name.to_string());
    }
}

impl<'a> Dispose for ContextContents<'a, DeclContents> {
    /// Ensure that whenever we leave a stack, we propagate
    /// all the necessary bindings to the parent.
    fn dispose(&mut self) {
        if let Some(ref parent) = self.parent {
            let mut parent = parent.borrow_mut();
            // Propagate everything.
            parent.data.var_names.extend(self.data.var_names.drain());
            parent.data.let_names.extend(self.data.let_names.drain());
            parent.data.const_names.extend(self.data.const_names.drain());
        }
    }
}

impl<'a> Context<'a, DeclContents> {
    pub fn add_var_name(&mut self, name: &str) {
        self.contents.borrow_mut().add_var_name(name)
    }
    pub fn add_let_name(&mut self, name: &str) {
        self.contents.borrow_mut().add_let_name(name);
    }
    pub fn add_const_name(&mut self, name: &str) {
        self.contents.borrow_mut().add_const_name(name);
    }
    /// Check whether the name is lexically bound somewhere on the stack.
    pub fn is_lex_bound(&self, name: &str) -> bool {
        let borrow = self.contents.borrow();
        borrow.is_lex_bound(name)
    }
    pub fn scope_kind(&self) -> ScopeKind {
        self.contents.borrow().scope_kind()
    }
    pub fn set_scope_kind(&mut self, kind: ScopeKind) {
        self.contents.borrow_mut().data.scope_kind = kind
    }
    pub fn clear_var_names(&mut self) {
        self.contents.borrow_mut()
            .data
            .var_names
            .clear();
    }
    pub fn clear_lex_names(&mut self) {
        self.contents.borrow_mut()
            .data
            .let_names
            .clear();
        self.contents.borrow_mut()
            .data
            .const_names
            .clear();
    }
    pub fn uses_strict(&self) -> bool {
        self.contents.borrow().uses_strict()
    }
    pub fn set_uses_strict(&mut self, value: bool) {
        self.contents.borrow_mut().set_uses_strict(value)
    }
    /// Store scope information in a field "BINJS:Scope" of the node.
    pub fn store(&self, parent: &mut Object) {
        assert!(!parent.contains_key(SCOPE_NAME));
        let borrow = self.contents.borrow();

        let mut var_decl_names: Vec<_> = borrow.data.var_names.iter().collect();
        var_decl_names.sort(); // To simplify testing (and hopefully improve compression).

        let mut let_decl_names: Vec<_> = borrow.data.let_names.iter().collect();
        let_decl_names.sort(); // To simplify testing (and hopefully improve compression)

        let mut const_decl_names: Vec<_> = borrow.data.const_names.iter().collect();
        const_decl_names.sort();

        parent.insert(SCOPE_NAME.to_string(), json!({
            "type": SCOPE_NAME,
            BINJS_VAR_NAME: var_decl_names,
            BINJS_LET_NAME: let_decl_names,
            BINJS_CONST_NAME: const_decl_names,
        }));
    }
}


pub trait Annotator {
    fn name(&self) -> String;

    /// At the end of this pass:
    /// - LexicallyDeclaredNames is correct;
    /// - VarDeclaredNames is correct;
    fn process_declarations(&self, me: &Annotator, ctx: &mut Context<DeclContents>, object: &mut Object) -> Result<(), ASTError>;
    fn process_declarations_aux(&self, me: &Annotator, ctx: &mut Context<DeclContents>, tree: &mut JSON) -> Result<(), ASTError> {
        // Only process object nodes.
        match *tree {
            JSON::Array(ref mut array) => {
                for tree in array.iter_mut() {
                    me.process_declarations_aux(me, ctx, tree)?
                }
            }
            JSON::Object(ref mut object) => {
                if let Ok(kind) = object.get_string("type", "Field `type`")
                    .map(str::to_string)
                {
                    let mut ctx = ctx.enter_obj(&kind)?;
                    me.process_declarations(me, &mut ctx, object)?
                } else {
                    // No type, skipping.
                }
            }
            _ => {
                // Not object/array, skipping.
            }
        }
        Ok(())
    }

    fn process_declarations_field(&self, me: &Annotator, ctx: &mut Context<DeclContents>, object: &mut Object, key: &str) -> Result<(), ASTError> {
        if let Some(ref mut json) = object.get_mut(key) {
            return me.process_declarations_aux(me, ctx, json)
        }
        Err(ASTError::InvalidValue {
            got: serde_json::to_string(object).unwrap(),
            expected: format!("Field `{}`", key)
        })
    }

    /// At the START of this pass:
    /// - LexicallyDeclaredNames MUST BE correct;
    /// - VarDeclaredNames MUST BE correct;
    ///
    /// At the END of this pass:
    /// - LexicallyDeclaredNames is correct;
    /// - VarDeclaredNames is correct;
    /// - CapturedNames is correct;
    /// - HasDirectEval is correct;
    fn process_references(&self, me: &Annotator, ctx: &mut Context<RefContents>, object: &mut Object) -> Result<(), ASTError>;
    fn process_references_aux(&self, me: &Annotator, ctx: &mut Context<RefContents>, tree: &mut JSON) -> Result<(), ASTError> {
        // Only process object nodes and array.
        match *tree {
            JSON::Array(ref mut array) => {
                for tree in array.iter_mut() {
                    me.process_references_aux(me, ctx, tree)?
                }
            }
            JSON::Object(ref mut object) => {
                if let Ok(kind) = object.get_string("type", "Field `type`")
                    .map(str::to_string)
                {
                    let mut ctx = ctx.enter_obj(&kind)?;
                    me.process_references(me, &mut ctx, object)?
                } else {
                    // No type, skipping.
                }
            }
            _ => {
                // Not object/array, skipping.
            }
        }
        Ok(())
    }

    fn process_references_field(&self, me: &Annotator, ctx: &mut Context<RefContents>, object: &mut Object, key: &str) -> Result<(), ASTError> {
        if let Some(ref mut json) = object.get_mut(key) {
            return me.process_references_aux(me, ctx, json)
        }
        Err(ASTError::InvalidValue {
            got: serde_json::to_string(object).unwrap(),
            expected: format!("Field `{}`", key)
        })
    }
}

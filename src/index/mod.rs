mod am;
mod build;
mod hook;
mod insert;
mod options;
mod scan;
mod vacuum;

pub fn init() {
    options::init();
    hook::init();
}

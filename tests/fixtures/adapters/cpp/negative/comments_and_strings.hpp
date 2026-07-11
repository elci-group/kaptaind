// Constructs hidden inside line comments are skipped.
// class Hidden {
// public:
//     void secret() {}
// };
// struct Ghost { int x; };
// namespace commented { void fn() {} }
const char* kText = "struct Nope { int x; }; class Fake {}";

// Constructors, destructor and operator overloads have bodies.
class Foo {
public:
    Foo() {}
    ~Foo() {}
    bool operator==(const Foo& o) { return true; }
};

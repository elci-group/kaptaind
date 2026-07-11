// A class with an inline method *definition* (body present).
class Counter {
public:
    void increment() {
        ++n_;
    }
private:
    int n_ = 0;
};

// Templates: the template line is ignored; the class is still seen.
template <typename T>
class Box {
public:
    T get() const { return value; }
private:
    T value{};
};

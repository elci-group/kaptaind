namespace MyApp
{
    class Holder
    {
        private string _name;

        private void Hidden() { }

        private int Secret { get; set; }

        void ImplicitPrivate() { }
    }
}

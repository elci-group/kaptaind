namespace MyApp
{
    internal class Service
    {
        internal void DoInternal() { }

        protected void DoProtected() { }

        protected internal void DoProtectedInternal() { }

        private protected void DoPrivateProtected() { }
    }
}

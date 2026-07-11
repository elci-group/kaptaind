using System.Collections.Generic;

namespace MyApp
{
    public class Repository<T> where T : class
    {
        public List<T> GetAll<T>() where T : class => new List<T>();

        public T Echo<T>(T input) => input;
    }
}

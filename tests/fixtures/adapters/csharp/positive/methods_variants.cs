using System.Collections.Generic;
using System.Threading.Tasks;

namespace MyApp
{
    public class Service
    {
        public static void Main(string[] args) { }

        public int Add(int a, int b) => a + b;

        public List<T> GetItems<T>() where T : class => new List<T>();

        public async Task<string> FetchAsync(string url) => await Task.FromResult(url);
    }
}

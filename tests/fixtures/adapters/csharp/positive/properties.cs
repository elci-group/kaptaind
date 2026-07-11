namespace MyApp
{
    public class Settings
    {
        public string Name { get; set; }

        public int Count { get; private set; }

        public double Ratio => 0.5;

        public static Settings Instance { get; }
    }
}

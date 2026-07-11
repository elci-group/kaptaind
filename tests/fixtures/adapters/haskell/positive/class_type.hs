class Eq a => Comparable a where
    compare :: a -> a -> Ordering

type Name = String

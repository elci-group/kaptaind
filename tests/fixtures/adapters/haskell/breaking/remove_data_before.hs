module Breaking (Result, combine) where

data Result a = Ok a | Err String

combine :: [a] -> [a] -> [a]
combine xs ys = xs ++ ys

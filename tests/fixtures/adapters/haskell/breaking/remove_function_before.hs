module Breaking (add, Result) where

add :: Int -> Int -> Int
add x y = x + y

data Result a = Ok a | Err String

defprotocol Enumerable do
  @doc "Reduces the collection."
  def reduce(collection, acc, fun)
end

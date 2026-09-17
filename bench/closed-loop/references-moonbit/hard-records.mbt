// task: hard-records — sum qty of items with qty > 4 over (3, 5, 8) -> 13
struct Item {
  name : String
  qty : Int
}

///|
fn main {
  let items : Array[Item] = [
    Item::{ name: "apple", qty: 3 },
    Item::{ name: "pear", qty: 5 },
    Item::{ name: "plum", qty: 8 },
  ]
  let total = items.filter(fn(it : Item) -> Bool {
    it.qty > 4
  }).fold(init=0, fn(acc : Int, it : Item) -> Int {
    acc + it.qty
  })
  println(total)
}

using System;
using System.IO;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class databaseHGB
{
	private string FileN;

	private int Col;

	private string Sep;

	private string[,] M;

	private int Counter;

	public bool Uniqueness;

	public string TableFileName => FileN;

	public string TableSeparator => Sep;

	public int TableColumns => Col;

	public int TableCounter => Counter;

	public string ItemValue
	{
		get
		{
			return Item(colum, row);
		}
		set
		{
			Item(colum, row, value);
		}
	}

	private void NewFolder(string d = "HGB")
	{
		d = "\\" + d + "\\";
		if (!Directory.Exists(All.MyDoc() + d))
		{
			Directory.CreateDirectory(All.MyDoc() + d);
		}
	}

	public databaseHGB()
	{
		FileN = All.MyDoc() + "\\HGB\\test.txt";
		Col = 1;
		Sep = "?";
		M = new string[2, 1];
		Counter = 0;
		Uniqueness = true;
		NewFolder();
		FileN = All.MyDoc() + "\\HGB\\test.txt";
	}

	public databaseHGB(string FileName)
	{
		FileN = All.MyDoc() + "\\HGB\\test.txt";
		Col = 1;
		Sep = "?";
		M = new string[2, 1];
		Counter = 0;
		Uniqueness = true;
		if (Operators.CompareString(FileName.Trim(), "", TextCompare: false) != 0)
		{
			FileN = FileName;
		}
	}

	public databaseHGB(string FileName, int Columns, string separator)
	{
		FileN = All.MyDoc() + "\\HGB\\test.txt";
		Col = 1;
		Sep = "?";
		M = new string[2, 1];
		Counter = 0;
		Uniqueness = true;
		checked
		{
			if (Operators.CompareString(FileName.Trim(), "", TextCompare: false) != 0)
			{
				FileN = FileName;
				Sep = separator;
				Col = Columns;
				M = new string[Col + 1, Counter + 1];
				int col = Col;
				for (int i = 0; i <= col; i++)
				{
					M[i, 0] = i.ToString();
				}
			}
		}
	}

	public databaseHGB(int Columns, string Separator)
	{
		FileN = All.MyDoc() + "\\HGB\\test.txt";
		Col = 1;
		Sep = "?";
		M = new string[2, 1];
		Counter = 0;
		Uniqueness = true;
		NewFolder();
		FileN = All.MyDoc() + "\\HGB\\test.txt";
		Sep = Separator;
		Col = Columns;
		checked
		{
			M = new string[Col + 1, Counter + 1];
			int col = Col;
			for (int i = 0; i <= col; i++)
			{
				M[i, 0] = i.ToString();
			}
		}
	}

	public databaseHGB(string FileName, int Columns)
	{
		FileN = All.MyDoc() + "\\HGB\\test.txt";
		Col = 1;
		Sep = "?";
		M = new string[2, 1];
		Counter = 0;
		Uniqueness = true;
		checked
		{
			if (Operators.CompareString(FileName.Trim(), "", TextCompare: false) != 0)
			{
				FileN = FileName;
				Col = Columns;
				M = new string[Col + 1, Counter + 1];
				int col = Col;
				for (int i = 0; i <= col; i++)
				{
					M[i, 0] = i.ToString();
				}
			}
		}
	}

	public int SumInt(int Column)
	{
		if (Column > Col)
		{
			return 0;
		}
		int num = 0;
		int counter = Counter;
		checked
		{
			for (int i = 1; i <= counter; i++)
			{
				if (Versioned.IsNumeric(M[Column, i]))
				{
					num += Convert.ToInt32(M[Column, i]);
				}
			}
			return num;
		}
	}

	public float SumSin(int Column)
	{
		if (Column > Col)
		{
			return 0f;
		}
		float num = 0f;
		int counter = Counter;
		for (int i = 1; i <= counter; i = checked(i + 1))
		{
			if (Versioned.IsNumeric(M[Column, i]))
			{
				num += Convert.ToSingle(M[Column, i]);
			}
		}
		return num;
	}

	public float SumSin(int ColumnSum, int ColumnSear, string Search)
	{
		if (ColumnSum > Col)
		{
			return 0f;
		}
		if (ColumnSear > Col)
		{
			return 0f;
		}
		float num = 0f;
		Search = Search.Trim();
		int counter = Counter;
		for (int i = 1; i <= counter; i = checked(i + 1))
		{
			if (Operators.CompareString(M[ColumnSear, i], Search, TextCompare: false) == 0 && Versioned.IsNumeric(M[ColumnSum, i]))
			{
				num += Convert.ToSingle(M[ColumnSum, i]);
			}
		}
		return num;
	}

	public int SumInt(int Column, int ColumnSear, string Search)
	{
		if (Column > Col)
		{
			return 0;
		}
		if (ColumnSear > Col)
		{
			return 0;
		}
		int num = 0;
		Search = Search.Trim();
		int counter = Counter;
		checked
		{
			for (int i = 1; i <= counter; i++)
			{
				if (Operators.CompareString(M[ColumnSear, i], Search, TextCompare: false) == 0 && Versioned.IsNumeric(M[Column, i]))
				{
					num += Convert.ToInt32(M[Column, i]);
				}
			}
			return num;
		}
	}

	public void Load()
	{
		bool flag = true;
		checked
		{
			try
			{
				StreamReader streamReader = new StreamReader(FileN);
				do
				{
					Array instance = streamReader.ReadLine().Split(Conversions.ToChar(Sep));
					if (!flag)
					{
						Counter++;
						ref string[,] m = ref M;
						m = (string[,])Utils.CopyArray(m, new string[Col + 1, Counter + 1]);
					}
					flag = false;
					int col = Col;
					for (int i = 0; i <= col; i++)
					{
						M[i, Counter] = Conversions.ToString(NewLateBinding.LateIndexGet(instance, new object[1] { i }, null));
					}
				}
				while (!streamReader.EndOfStream);
				streamReader.Close();
				streamReader = null;
			}
			catch (IOException ex)
			{
				ProjectData.SetProjectError(ex);
				IOException ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	public bool AddRow(string text)
	{
		if (Uniqueness & (Search(text) > -1))
		{
			return false;
		}
		checked
		{
			Counter++;
			ref string[,] m = ref M;
			m = (string[,])Utils.CopyArray(m, new string[Col + 1, Counter + 1]);
			M[0, Counter] = text.Trim();
			return true;
		}
	}

	public bool RemoveRow(int index)
	{
		if (index > Counter || index < 1)
		{
			return false;
		}
		checked
		{
			int num = Counter - 1;
			for (int i = index; i <= num; i++)
			{
				int col = Col;
				for (int j = 0; j <= col; j++)
				{
					M[j, i] = M[j, i + 1];
				}
			}
			Counter--;
			ref string[,] m = ref M;
			m = (string[,])Utils.CopyArray(m, new string[Col + 1, Counter + 1]);
			return true;
		}
	}

	public string Item(int colum, int row, string text)
	{
		if (row < 1)
		{
			return "";
		}
		if ((colum > Col) | (row > Counter))
		{
			return "";
		}
		if ((colum == 0) & Uniqueness)
		{
			if (Search(text) < 0)
			{
				M[colum, row] = text.Trim();
				return text;
			}
			return "";
		}
		M[colum, row] = text.Trim();
		return text;
	}

	public string Item(int colum, int row)
	{
		if (row < 1)
		{
			return "";
		}
		if ((colum > Col) | (row > Counter))
		{
			return "";
		}
		return M[colum, row];
	}

	public int Search(string text, int n)
	{
		int counter = Counter;
		for (int i = n; i <= counter; i = checked(i + 1))
		{
			if (Operators.CompareString(M[0, i], text.Trim(), TextCompare: false) == 0)
			{
				return i;
			}
		}
		return -1;
	}

	public int Search(string text, int colu, int n)
	{
		int counter = Counter;
		for (int i = n; i <= counter; i = checked(i + 1))
		{
			if (Operators.CompareString(M[colu, i], text.Trim(), TextCompare: false) == 0)
			{
				return i;
			}
		}
		return -1;
	}

	public int Search(string text)
	{
		int counter = Counter;
		for (int i = 0; i <= counter; i = checked(i + 1))
		{
			if (Operators.CompareString(M[0, i], text.Trim(), TextCompare: false) == 0)
			{
				return i;
			}
		}
		return -1;
	}

	public void Save()
	{
		checked
		{
			try
			{
				StreamWriter streamWriter = new StreamWriter(FileN);
				string text = "";
				int counter = Counter;
				for (int i = 0; i <= counter; i++)
				{
					text = "";
					int col = Col;
					for (int j = 0; j <= col; j++)
					{
						text += M[j, i];
						if (j < Col)
						{
							text += Sep;
						}
					}
					streamWriter.WriteLine(text);
				}
				streamWriter.Flush();
				streamWriter.Close();
				streamWriter = null;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	public string NameColumn(int n)
	{
		if (n > Col)
		{
			return "";
		}
		return M[n, 0];
	}

	public string NameColumn(int n, string name)
	{
		if (n > Col)
		{
			return "";
		}
		M[n, 0] = name.Trim();
		return M[n, 0];
	}
}

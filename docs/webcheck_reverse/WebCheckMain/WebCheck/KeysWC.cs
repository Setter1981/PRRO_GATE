using System;
using System.IO;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class KeysWC
{
	private const int minNomerov = 27;

	private readonly XmlDocument X;

	private readonly Wcwc Kod;

	private readonly Random Rnd;

	public string type;

	private string xmlS;

	private string[,] tov;

	private int Z;

	private string Tin;

	private byte[,] M;

	private byte[] MF;

	public KeysWC()
	{
		X = new XmlDocument();
		Kod = new Wcwc();
		Rnd = new Random();
		type = "test";
		tov = new string[4, 1];
		Z = 0;
		Tin = "";
		M = new byte[21, 1];
		MF = new byte[1];
	}

	internal string KEYs(string value)
	{
		string result;
		try
		{
			WebCheck.All.NewFolder();
			xmlS = value.Trim().ToLower();
			X.LoadXml(xmlS);
			string text = SearchFn().Trim();
			if (text.Length == 10)
			{
				result = (Dekode() ? Search(text) : "");
			}
			else
			{
				Tin = X.SelectSingleNode("/org/@tin").Value;
				Tin = Tin.Trim();
				All();
				result = "";
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			xmlS = "";
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string SearchFn()
	{
		string result;
		try
		{
			string? value = X.SelectSingleNode("/checklic/@fn").Value;
			Tin = X.SelectSingleNode("/checklic/@tin").Value;
			Tin = Tin.Trim();
			result = value;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string Search(string fn)
	{
		string result = "fn=0000000000 date=00.00.0000 tarif=0";
		int z = Z;
		for (int i = 0; i <= z; i = checked(i + 1))
		{
			if (Operators.CompareString(tov[0, i], fn, false) == 0)
			{
				result = "fn=" + tov[0, i] + " date=" + tov[1, i] + " tarif=" + tov[2, i];
				break;
			}
		}
		return result;
	}

	internal long DhgbK(string tinS, string fnS)
	{
		Tin = tinS.Trim();
		if (!Dekode())
		{
			return 0L;
		}
		return SearchDate(fnS);
	}

	private long SearchDate(string fn)
	{
		long result = 0L;
		int z = Z;
		for (int i = 0; i <= z; i = checked(i + 1))
		{
			if (Operators.CompareString(tov[0, i], fn, false) == 0)
			{
				result = DateStrToInt64(tov[1, i]);
				break;
			}
		}
		return result;
	}

	internal long DateStrToInt64(string Dstr)
	{
		if (Dstr.Length == 10)
		{
			return Conversions.ToInteger(string.Concat(string.Concat(Conversions.ToString(Dstr[6]) + Conversions.ToString(Dstr[7]) + Conversions.ToString(Dstr[8]) + Conversions.ToString(Dstr[9]), Conversions.ToString(Dstr[3]), Conversions.ToString(Dstr[4])), Conversions.ToString(Dstr[0]), Conversions.ToString(Dstr[1])));
		}
		return 0L;
	}

	private bool All()
	{
		checked
		{
			bool result;
			try
			{
				XmlNodeList elementsByTagName = X.GetElementsByTagName("point");
				Z = elementsByTagName.Count - 1;
				int z = Z;
				if (Z < 27)
				{
					Z += 27 - Z;
				}
				tov = new string[3, Z + 1];
				M = new byte[21, Z + 1];
				MF = new byte[21 * (Z + 1) - 1 + 1];
				if (Reg())
				{
					int z2 = Z;
					for (int i = 0; i <= z2; i++)
					{
						if (i <= z)
						{
							XmlDocument xmlDocument = new XmlDocument();
							xmlDocument.LoadXml(elementsByTagName[i].OuterXml);
							tov[0, i] = xmlDocument.SelectSingleNode("/point/@fn").Value;
							tov[1, i] = xmlDocument.SelectSingleNode("/point/@licenddate").Value;
							tov[2, i] = xmlDocument.SelectSingleNode("/point/@tarif").Value;
						}
						else
						{
							tov[0, i] = LFN();
							tov[1, i] = LDate();
							tov[2, i] = "0";
						}
					}
				}
				else
				{
					int z3 = Z;
					for (int i = 0; i <= z3; i++)
					{
						new XmlDocument().LoadXml(elementsByTagName[i].OuterXml);
						tov[0, i] = LFN();
						tov[1, i] = LDate();
						tov[2, i] = "0";
					}
				}
				result = Kode();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private string LFN()
	{
		string text = "";
		int num = 0;
		do
		{
			text += Rnd.Next(9);
			num = checked(num + 1);
		}
		while (num <= 9);
		return text;
	}

	private string LDate()
	{
		string text = "";
		int num = 0;
		checked
		{
			do
			{
				text += Rnd.Next(9);
				num++;
			}
			while (num <= 1);
			text += ".";
			num = 0;
			do
			{
				text += Rnd.Next(9);
				num++;
			}
			while (num <= 1);
			text += ".";
			num = 0;
			do
			{
				text += Rnd.Next(9);
				num++;
			}
			while (num <= 3);
			return text;
		}
	}

	private bool Kode()
	{
		int z = Z;
		checked
		{
			for (int i = 0; i <= z; i++)
			{
				M[0, i] = (byte)Rnd.Next(10);
				string text = tov[0, i];
				M[1, i] = Conversions.ToByte(text[6].ToString());
				M[2, i] = Conversions.ToByte(text[2].ToString());
				M[3, i] = Conversions.ToByte(text[5].ToString());
				string text2 = tov[2, i];
				M[4, i] = Conversions.ToByte(text2[0].ToString());
				M[5, i] = Conversions.ToByte(text[0].ToString());
				M[6, i] = Conversions.ToByte(text[1].ToString());
				M[7, i] = Conversions.ToByte(text[8].ToString());
				M[8, i] = Conversions.ToByte(text[7].ToString());
				string text3 = tov[1, i];
				M[9, i] = Conversions.ToByte(text3[0].ToString());
				M[10, i] = Conversions.ToByte(text3[1].ToString());
				M[11, i] = Conversions.ToByte(text[9].ToString());
				M[12, i] = Conversions.ToByte(text3[3].ToString());
				M[13, i] = Conversions.ToByte(text3[4].ToString());
				M[14, i] = Conversions.ToByte(text[4].ToString());
				M[15, i] = Conversions.ToByte(text3[8].ToString());
				M[16, i] = (byte)Rnd.Next(10);
				M[17, i] = Conversions.ToByte(text3[9].ToString());
				if (Conversions.ToInteger(text2) > 0)
				{
					M[18, i] = (byte)Rnd.Next(5, 10);
				}
				else
				{
					M[18, i] = (byte)Rnd.Next(5);
				}
				M[19, i] = Conversions.ToByte(text[3].ToString());
				M[20, i] = SumByte(i);
			}
			int z2 = Z;
			for (int i = 0; i <= z2; i++)
			{
				int num = 0;
				do
				{
					byte keyS = Conversions.ToByte(num.ToString()[0].ToString());
					M[num, i] = Kod.Hgb(M[num, i], decoding: false, keyS);
					MF[i * 21 + num] = M[num, i];
					num++;
				}
				while (num <= 20);
			}
			return SaveByte();
		}
	}

	private bool Dekode()
	{
		if (!LoadByte())
		{
			return false;
		}
		int z = Z;
		checked
		{
			for (int i = 0; i <= z; i++)
			{
				int num = 0;
				do
				{
					byte keyS = Conversions.ToByte(num.ToString()[0].ToString());
					M[num, i] = Kod.Hgb(M[num, i], decoding: true, keyS);
					num++;
				}
				while (num <= 20);
				if (M[20, i] != SumByte(i))
				{
					M[18, i] = 0;
				}
			}
			int z2 = Z;
			for (int i = 0; i <= z2; i++)
			{
				if (M[18, i] > 4)
				{
					tov[0, i] = M[5, i].ToString() + M[6, i] + M[2, i] + M[19, i] + M[14, i];
					ref string reference = ref tov[0, i];
					reference = reference + M[3, i] + M[1, i] + M[8, i] + M[7, i] + M[11, i];
					tov[1, i] = M[9, i].ToString() + M[10, i] + "." + M[12, i] + M[13, i] + ".20" + M[15, i] + M[17, i];
					tov[2, i] = M[4, i].ToString();
				}
				else
				{
					tov[0, i] = "";
					tov[1, i] = "";
					tov[2, i] = "";
				}
			}
			return true;
		}
	}

	private byte SumByte(int value)
	{
		string text = "";
		int num = 2;
		do
		{
			text += M[num, value];
			num = checked(num + 1);
		}
		while (num <= 17);
		return Conversions.ToByte(StrByteToByte(text));
	}

	private string StrByteToByte(string value)
	{
		int num = 0;
		checked
		{
			int num2 = value.Length - 1;
			for (int i = 0; i <= num2; i++)
			{
				num += Conversions.ToInteger(value[i].ToString());
			}
			string text = num.ToString();
			if (text.Length > 1)
			{
				return StrByteToByte(text);
			}
			return text;
		}
	}

	private bool Reg()
	{
		string folderPath = Environment.GetFolderPath(Environment.SpecialFolder.Personal);
		IniHGB iniHGB = new IniHGB(folderPath + "\\test.ini");
		if (Operators.CompareString(type, "WebCheck", false) == 0)
		{
			if (Operators.CompareString(iniHGB.GetString("wc", "wc"), "WebCheck", false) != 0)
			{
				return false;
			}
			return true;
		}
		return false;
	}

	private bool SaveByte()
	{
		return true;
	}

	private bool LoadByte()
	{
		string path = WebCheck.All.MyDoc() + "\\WebCheck\\Lic\\" + Tin + ".lic";
		bool result;
		checked
		{
			if (File.Exists(path))
			{
				byte[] array = File.ReadAllBytes(path);
				Z = (int)Math.Round((double)array.Length / 21.0) - 1;
				tov = new string[3, Z + 1];
				M = new byte[21, Z + 1];
				try
				{
					int z = Z;
					for (int i = 0; i <= z; i++)
					{
						int num = 0;
						do
						{
							M[num, i] = array[i * 21 + num];
							num++;
						}
						while (num <= 20);
					}
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (WebCheck.All.A.Status)
					{
						WebCheck.All.Lg.SaveTextToLog("Ошибка файла лицензии", "Неверный формат файла лицензии", "Проверьте доступ к интернету и настройки брандмауэра");
					}
					result = false;
					ProjectData.ClearProjectError();
					goto IL_00ed;
				}
				result = true;
			}
			else
			{
				result = false;
			}
			goto IL_00ed;
		}
		IL_00ed:
		return result;
	}
}

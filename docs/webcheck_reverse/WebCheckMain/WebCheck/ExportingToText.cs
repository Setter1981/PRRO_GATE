using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class ExportingToText : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("CBY")]
	private ComboBox _CBY;

	[CompilerGenerated]
	[AccessedThroughProperty("ExportB")]
	private Button _ExportB;

	[CompilerGenerated]
	[AccessedThroughProperty("BDir")]
	private Button _BDir;

	private string eFN;

	private ExportTextToFile ETF;

	internal virtual ComboBox CBY
	{
		[CompilerGenerated]
		get
		{
			return _CBY;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CBY_SelectedIndexChanged;
			ComboBox cBY = _CBY;
			if (cBY != null)
			{
				cBY.SelectedIndexChanged -= eventHandler;
			}
			_CBY = value;
			cBY = _CBY;
			if (cBY != null)
			{
				cBY.SelectedIndexChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxS")]
	internal virtual TextBox TextBoxS
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ExportB
	{
		[CompilerGenerated]
		get
		{
			return _ExportB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ExportB_Click;
			Button exportB = _ExportB;
			if (exportB != null)
			{
				((Control)exportB).Click -= eventHandler;
			}
			_ExportB = value;
			exportB = _ExportB;
			if (exportB != null)
			{
				((Control)exportB).Click += eventHandler;
			}
		}
	}

	internal virtual Button BDir
	{
		[CompilerGenerated]
		get
		{
			return _BDir;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = BDir_Click;
			Button bDir = _BDir;
			if (bDir != null)
			{
				((Control)bDir).Click -= eventHandler;
			}
			_BDir = value;
			bDir = _BDir;
			if (bDir != null)
			{
				((Control)bDir).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0070: Unknown result type (might be due to invalid IL or missing references)
		//IL_007a: Expected O, but got Unknown
		//IL_0100: Unknown result type (might be due to invalid IL or missing references)
		//IL_010a: Expected O, but got Unknown
		//IL_018d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0197: Expected O, but got Unknown
		//IL_0215: Unknown result type (might be due to invalid IL or missing references)
		//IL_021f: Expected O, but got Unknown
		//IL_02b5: Unknown result type (might be due to invalid IL or missing references)
		//IL_02bf: Expected O, but got Unknown
		//IL_03b3: Unknown result type (might be due to invalid IL or missing references)
		//IL_03bd: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(ExportingToText));
		CBY = new ComboBox();
		TextBoxS = new TextBox();
		ExportB = new Button();
		BDir = new Button();
		Label1 = new Label();
		((Control)this).SuspendLayout();
		CBY.DropDownStyle = (ComboBoxStyle)2;
		((Control)CBY).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)CBY).FormattingEnabled = true;
		((Control)CBY).Location = new Point(12, 12);
		((Control)CBY).Name = "CBY";
		((Control)CBY).Size = new Size(170, 33);
		((Control)CBY).TabIndex = 2;
		((TextBoxBase)TextBoxS).BackColor = SystemColors.Window;
		((Control)TextBoxS).Enabled = false;
		((Control)TextBoxS).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxS).Location = new Point(12, 62);
		TextBoxS.Multiline = true;
		((Control)TextBoxS).Name = "TextBoxS";
		((TextBoxBase)TextBoxS).ReadOnly = true;
		((Control)TextBoxS).Size = new Size(598, 121);
		((Control)TextBoxS).TabIndex = 12;
		TextBoxS.TextAlign = (HorizontalAlignment)2;
		((Control)ExportB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ExportB).Location = new Point(12, 199);
		((Control)ExportB).Name = "ExportB";
		((Control)ExportB).Size = new Size(598, 47);
		((Control)ExportB).TabIndex = 14;
		((ButtonBase)ExportB).Text = "Почати експорт";
		((ButtonBase)ExportB).UseVisualStyleBackColor = true;
		((Control)BDir).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)BDir).Location = new Point(188, 94);
		((Control)BDir).Name = "BDir";
		((Control)BDir).Size = new Size(410, 47);
		((Control)BDir).TabIndex = 15;
		((ButtonBase)BDir).Text = "Відкрити папку з файлами";
		((ButtonBase)BDir).UseVisualStyleBackColor = true;
		((Control)BDir).Visible = false;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(268, 9);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(284, 48);
		((Control)Label1).TabIndex = 16;
		Label1.Text = "Чеки експортуються до папки:\r\n     Мої документи\\WebCheck";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(622, 263);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)BDir);
		((Control)this).Controls.Add((Control)(object)ExportB);
		((Control)this).Controls.Add((Control)(object)TextBoxS);
		((Control)this).Controls.Add((Control)(object)CBY);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "ExportingToText";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Експорт чеків за період";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public ExportingToText(string eF)
	{
		((Form)this).Load += ExportingToText_Load;
		ETF = new ExportTextToFile();
		InitializeComponent();
		eFN = eF;
	}

	private void ExportingToText_Load(object sender, EventArgs e)
	{
		int year = DateTime.Now.Year;
		for (int i = year; i >= 2021; i = checked(i + -1))
		{
			CBY.Items.Add((object)i.ToString());
		}
		CBY.Text = year.ToString();
		TextBoxS.Text = "Експорт чеків ПРРО ФН " + eFN + " за " + year + " рік";
	}

	private void CBY_SelectedIndexChanged(object sender, EventArgs e)
	{
		TextBoxS.Text = "Експорт чеків ПРРО ФН " + eFN + " за " + CBY.SelectedItem.ToString() + " рік";
	}

	private void ExportB_Click(object sender, EventArgs e)
	{
		int num = 0;
		((Control)ExportB).Enabled = false;
		((Control)CBY).Enabled = false;
		DirNewArh();
		ShiftAll shiftAll = new ShiftAll(CBY.SelectedItem.ToString());
		string text = "";
		checked
		{
			if (shiftAll.ShiftsYear > 0)
			{
				int shiftsYear = shiftAll.ShiftsYear;
				for (int i = 1; i <= shiftsYear; i++)
				{
					string text2 = "-" + eFN + "-" + Strings.Replace(shiftAll.get_Seller(1, i), ".", "_", 1, -1, (CompareMethod)0);
					text2 = Strings.Replace(text2, " ", "_", 1, -1, (CompareMethod)0);
					text2 = Strings.Replace(text2, ":", "_", 1, -1, (CompareMethod)0);
					text = "\\" + shiftAll.get_Seller(0, i) + text2 + ".txt";
					ETF.PathFile = MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString() + text;
					string text3 = ETF.NewFile();
					if (Operators.CompareString(text3, "", false) != 0)
					{
						TextBoxS.Text = "Помилка видалення файлу " + text3;
						((Control)ExportB).Enabled = true;
						((Control)CBY).Enabled = true;
						return;
					}
					CheckShift checkShift = new CheckShift(shiftAll.get_Seller(0, i));
					int checksShift = checkShift.ChecksShift;
					for (int j = 1; j <= checksShift; j++)
					{
						if (j != checkShift.ChecksShift)
						{
							TextBoxS.Text = " Eкспорт зміни № " + shiftAll.get_Seller(0, i) + "       чек: " + checkShift.get_Seller(0, j) + Environment.NewLine + " Залишилось змін: " + (1 + shiftAll.ShiftsYear - i);
						}
						else
						{
							TextBoxS.Text = " Eкспорт зміни № " + shiftAll.get_Seller(0, i) + "       чек: " + checkShift.get_Seller(0, j) + Environment.NewLine + " Залишилось змін: " + (shiftAll.ShiftsYear - i);
						}
						SaveChecktToFile(checkShift.get_Seller(0, j));
						num++;
						Application.DoEvents();
					}
				}
				TextBox textBoxS;
				(textBoxS = TextBoxS).Text = textBoxS.Text + Environment.NewLine + "---------" + Environment.NewLine + " ЕКСПОРТ ВИКОНАНО!!!   Опрацьовано чеків: " + num;
			}
			((Control)ExportB).Enabled = true;
			((Control)CBY).Enabled = true;
		}
	}

	private string MyDocPath()
	{
		return Environment.GetFolderPath(Environment.SpecialFolder.Personal);
	}

	private string SaveChecktToFile(string eN)
	{
		All.A.ExportLength = 30;
		CheckStamp checkStamp = new CheckStamp();
		if (checkStamp.CheckXML(eN).Length == 0)
		{
			ETF.NewFile();
			return "Помилка отримання чека: " + eN;
		}
		checked
		{
			int num = checkStamp.SizeCheck() - 1;
			for (int i = 0; i <= num; i++)
			{
				object obj = ETF.SaveTextToFile(checkStamp.LineFromCheck(i));
				if (Operators.ConditionalCompareObjectNotEqual(obj, (object)"", false))
				{
					ETF.NewFile();
					return Conversions.ToString(Operators.ConcatenateObject((object)"Помилка запису у файл ", obj));
				}
			}
			ETF.SaveTextToFile("");
			ETF.SaveTextToFile("");
			ETF.SaveTextToFile("");
			return "";
		}
	}

	private void DirNewArh()
	{
		if (!Directory.Exists(MyDocPath() + "\\WebCheck\\"))
		{
			Directory.CreateDirectory(MyDocPath() + "\\WebCheck\\");
		}
		if (!Directory.Exists(MyDocPath() + "\\WebCheck\\" + eFN + "\\"))
		{
			Directory.CreateDirectory(MyDocPath() + "\\WebCheck\\" + eFN + "\\");
		}
		if (!Directory.Exists(MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString() + "\\"))
		{
			Directory.CreateDirectory(MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString() + "\\");
		}
	}

	private void BDir_Click(object sender, EventArgs e)
	{
		DirNewArh();
		Interaction.Shell("explorer.exe " + MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString(), (AppWinStyle)2, false, -1);
	}
}

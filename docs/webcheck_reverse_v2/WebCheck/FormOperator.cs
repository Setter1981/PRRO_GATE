using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormOperator : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("CBR")]
	private CheckBox _CBR;

	private readonly bool AddOperator;

	private int indexOp;

	private string tFioOp;

	private string tInnOp;

	private string tPasOp;

	private string tKeyOp;

	internal virtual Button KeyB
	{
		[CompilerGenerated]
		get
		{
			return _KeyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click -= value2;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label11")]
	internal virtual Label Label11
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label9")]
	internal virtual Label Label9
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PasOpT")]
	internal virtual TextBox PasOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KeyOpT")]
	internal virtual TextBox KeyOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("InnOpT")]
	internal virtual TextBox InnOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("FioOpT")]
	internal virtual TextBox FioOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label8")]
	internal virtual Label Label8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
			}
		}
	}

	internal virtual CheckBox CBR
	{
		[CompilerGenerated]
		get
		{
			return _CBR;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CBR_CheckedChanged;
			CheckBox cBR = _CBR;
			if (cBR != null)
			{
				cBR.CheckedChanged -= value2;
			}
			_CBR = value;
			cBR = _CBR;
			if (cBR != null)
			{
				cBR.CheckedChanged += value2;
			}
		}
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormOperator));
		this.KeyB = new System.Windows.Forms.Button();
		this.Label11 = new System.Windows.Forms.Label();
		this.Label10 = new System.Windows.Forms.Label();
		this.Label9 = new System.Windows.Forms.Label();
		this.PasOpT = new System.Windows.Forms.TextBox();
		this.KeyOpT = new System.Windows.Forms.TextBox();
		this.InnOpT = new System.Windows.Forms.TextBox();
		this.FioOpT = new System.Windows.Forms.TextBox();
		this.Label8 = new System.Windows.Forms.Label();
		this.NoB = new System.Windows.Forms.Button();
		this.OkB = new System.Windows.Forms.Button();
		this.CBR = new System.Windows.Forms.CheckBox();
		base.SuspendLayout();
		this.KeyB.Location = new System.Drawing.Point(222, 114);
		this.KeyB.Name = "KeyB";
		this.KeyB.Size = new System.Drawing.Size(53, 30);
		this.KeyB.TabIndex = 25;
		this.KeyB.TabStop = false;
		this.KeyB.Text = "...";
		this.KeyB.UseVisualStyleBackColor = true;
		this.Label11.AutoSize = true;
		this.Label11.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label11.Location = new System.Drawing.Point(32, 157);
		this.Label11.Name = "Label11";
		this.Label11.Size = new System.Drawing.Size(202, 25);
		this.Label11.TabIndex = 33;
		this.Label11.Text = "Пароль ключа ЕЦП *";
		this.Label10.AutoSize = true;
		this.Label10.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label10.Location = new System.Drawing.Point(32, 117);
		this.Label10.Name = "Label10";
		this.Label10.Size = new System.Drawing.Size(121, 25);
		this.Label10.TabIndex = 32;
		this.Label10.Text = "Ключ ЕЦП *";
		this.Label9.AutoSize = true;
		this.Label9.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label9.Location = new System.Drawing.Point(32, 77);
		this.Label9.Name = "Label9";
		this.Label9.Size = new System.Drawing.Size(159, 25);
		this.Label9.TabIndex = 27;
		this.Label9.Text = "ІНН оператора *";
		this.PasOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PasOpT.Location = new System.Drawing.Point(293, 154);
		this.PasOpT.Name = "PasOpT";
		this.PasOpT.Size = new System.Drawing.Size(430, 30);
		this.PasOpT.TabIndex = 31;
		this.PasOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.KeyOpT.Enabled = false;
		this.KeyOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.KeyOpT.Location = new System.Drawing.Point(293, 114);
		this.KeyOpT.Name = "KeyOpT";
		this.KeyOpT.Size = new System.Drawing.Size(430, 30);
		this.KeyOpT.TabIndex = 30;
		this.KeyOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.InnOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.InnOpT.Location = new System.Drawing.Point(293, 74);
		this.InnOpT.Name = "InnOpT";
		this.InnOpT.Size = new System.Drawing.Size(430, 30);
		this.InnOpT.TabIndex = 29;
		this.InnOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.FioOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FioOpT.Location = new System.Drawing.Point(293, 33);
		this.FioOpT.Name = "FioOpT";
		this.FioOpT.Size = new System.Drawing.Size(430, 30);
		this.FioOpT.TabIndex = 28;
		this.FioOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label8.AutoSize = true;
		this.Label8.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label8.Location = new System.Drawing.Point(32, 36);
		this.Label8.Name = "Label8";
		this.Label8.Size = new System.Drawing.Size(159, 25);
		this.Label8.TabIndex = 26;
		this.Label8.Text = "ПІБ оператора *";
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(37, 218);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(132, 40);
		this.NoB.TabIndex = 38;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(591, 218);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(132, 40);
		this.OkB.TabIndex = 37;
		this.OkB.Text = "Ок";
		this.OkB.UseVisualStyleBackColor = true;
		this.CBR.AutoSize = true;
		this.CBR.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CBR.Location = new System.Drawing.Point(293, 217);
		this.CBR.Name = "CBR";
		this.CBR.Size = new System.Drawing.Size(260, 44);
		this.CBR.TabIndex = 39;
		this.CBR.Text = "Вказати відкладений ключ\r\nдля цього оператора";
		this.CBR.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(756, 279);
		base.Controls.Add(this.CBR);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.KeyB);
		base.Controls.Add(this.Label11);
		base.Controls.Add(this.Label10);
		base.Controls.Add(this.Label9);
		base.Controls.Add(this.PasOpT);
		base.Controls.Add(this.KeyOpT);
		base.Controls.Add(this.InnOpT);
		base.Controls.Add(this.FioOpT);
		base.Controls.Add(this.Label8);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormOperator";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "FormOperator";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormOperator(bool NewOperator, string IND = "0", string FIO = "", string INN = "", string PASS = "", string KEY = "")
	{
		base.Load += FormOperator_Load;
		InitializeComponent();
		Coding coding = new Coding();
		AddOperator = NewOperator;
		if (!Versioned.IsNumeric(IND))
		{
			IND = "0";
		}
		indexOp = Conversions.ToInteger(IND);
		tFioOp = FIO;
		tInnOp = INN;
		tPasOp = coding.DeCod(PASS);
		tKeyOp = KEY;
	}

	private void FormOperator_Load(object sender, EventArgs e)
	{
		base.CancelButton = NoB;
		base.AcceptButton = OkB;
		if (AddOperator)
		{
			Text = "Додати нового оператора";
			FioOpT.Enabled = true;
			InnOpT.Enabled = true;
			PasOpT.Enabled = true;
			KeyOpT.Enabled = false;
			CBR.Enabled = false;
		}
		else
		{
			Text = "Редагувати оператора";
			FioOpT.Enabled = true;
			InnOpT.Enabled = false;
			PasOpT.Enabled = true;
			KeyOpT.Enabled = false;
			FioOpT.Text = tFioOp;
			InnOpT.Text = tInnOp;
			PasOpT.Text = "*********";
			KeyOpT.Text = tKeyOp;
			if (Operators.CompareString(tInnOp[0].ToString(), "R", TextCompare: false) == 0)
			{
				CBR.Enabled = false;
			}
			else
			{
				CBR.Enabled = true;
			}
		}
		Show();
		FioOpT.Focus();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (AddOperator)
		{
			if (Operators.CompareString(FioOpT.Text.Trim(), "", TextCompare: false) == 0)
			{
				FioOpT.Focus();
			}
			else if (Operators.CompareString(InnOpT.Text.Trim(), "", TextCompare: false) == 0)
			{
				InnOpT.Focus();
			}
			else if (Operators.CompareString(KeyOpT.Text.Trim(), "", TextCompare: false) == 0)
			{
				string left = PathKey();
				if (Operators.CompareString(left, "", TextCompare: false) != 0)
				{
					KeyOpT.Text = left;
					PasOpT.Focus();
				}
			}
			else if (Operators.CompareString(PasOpT.Text.Trim(), "", TextCompare: false) == 0)
			{
				PasOpT.Focus();
			}
			else if (new OperatorsAll().CountOperators(InnOpT.Text.Trim()) > 0)
			{
				Interaction.MsgBox("Оператор з таким ІНН вже є!", MsgBoxStyle.Exclamation, "Новий оператор");
			}
			else if (All.l.AddNewOperator(All.l.TextToTextXML(FioOpT.Text), KeyOpT.Text, PasOpT.Text, InnOpT.Text))
			{
				Application.DoEvents();
				Close();
			}
		}
		else if (CBR.Checked)
		{
			if (new OperatorsAll().CountOperators("R" + InnOpT.Text.Trim()) > 0)
			{
				Interaction.MsgBox("Відкладений ключ вже є!", MsgBoxStyle.Exclamation, "Новий оператор");
			}
			else if (Operators.CompareString(KeyOpT.Text.Trim(), "", TextCompare: false) == 0)
			{
				string left2 = PathKey();
				if (Operators.CompareString(left2, "", TextCompare: false) != 0)
				{
					KeyOpT.Text = left2;
					PasOpT.Focus();
				}
			}
			else if (Operators.CompareString(PasOpT.Text.Trim(), "", TextCompare: false) == 0)
			{
				PasOpT.Focus();
			}
			else if (All.l.AddNewOperator(All.l.TextToTextXML("Відкладений ключ"), KeyOpT.Text, PasOpT.Text, "R" + InnOpT.Text))
			{
				Application.DoEvents();
				Close();
			}
		}
		else
		{
			UpdateInfa updateInfa = new UpdateInfa();
			if (Operators.CompareString(FioOpT.Text.Trim(), tFioOp, TextCompare: false) != 0)
			{
				string newInfa = FioOpT.Text.Trim();
				updateInfa.UPDATE("OPERATORS", "OPERATORNAME", Conversions.ToString(indexOp), newInfa);
			}
			if (Operators.CompareString(PasOpT.Text.Trim(), "*********", TextCompare: false) != 0 && Operators.CompareString(PasOpT.Text.Trim(), tPasOp, TextCompare: false) != 0)
			{
				string newInfa = PasOpT.Text.Trim();
				newInfa = new Coding().Cod(newInfa);
				updateInfa.UPDATE("OPERATORS", "KEYPASS", Conversions.ToString(indexOp), newInfa);
			}
			if (Operators.CompareString(KeyOpT.Text.Trim(), tKeyOp, TextCompare: false) != 0)
			{
				string newInfa = KeyOpT.Text.Trim();
				updateInfa.UPDATE("OPERATORS", "KEYPATH", Conversions.ToString(indexOp), newInfa);
			}
			IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini");
			iniHGB.WriteString(tInnOp, "EndKey", "");
			iniHGB.WriteString(tInnOp, "Updated", "0");
			Close();
		}
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string left = PathKey();
		if (Operators.CompareString(left, "", TextCompare: false) != 0)
		{
			KeyOpT.Text = left;
			PasOpT.Focus();
		}
	}

	private string PathKey()
	{
		OpenFileDialog openFileDialog = new OpenFileDialog();
		openFileDialog.Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if (openFileDialog.ShowDialog() == DialogResult.OK)
		{
			return openFileDialog.FileName;
		}
		return "";
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void CBR_CheckedChanged(object sender, EventArgs e)
	{
		if (CBR.Checked)
		{
			Text = "Відкладений ключ для оператора";
			FioOpT.Enabled = false;
			InnOpT.Enabled = false;
			PasOpT.Enabled = true;
			KeyOpT.Enabled = false;
			KeyOpT.Text = "";
			PasOpT.Text = "";
		}
		else
		{
			Text = "Редагувати оператора";
			FioOpT.Enabled = true;
			InnOpT.Enabled = false;
			PasOpT.Enabled = true;
			KeyOpT.Enabled = false;
			KeyOpT.Text = tKeyOp;
			PasOpT.Text = "*********";
		}
	}
}
